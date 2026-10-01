//! 线性规划求解：唯一后端 HiGHS。
//!
//! # 为什么删掉了 Ruiz 缩放 / clarabel / microlp
//!
//! 这里曾经是「Ruiz 均衡缩放 + clarabel（内点法）+ microlp（单纯形）」双后端，
//! 外加拿「最大取值 × 比例」剪枝、并按 `[1e-7, 1e-9, 1e-11, 0]` **从紧到松扫**
//! 的启发式。绕这么大圈的原因是：
//!
//! - microlp 在「列高度相似、系数跨二十个数量级」的大 LP 上会报 `Singular
//!   matrix`，把有解误报成不可解；
//! - clarabel 是内点法，给的是最优面的**解析中心**而不是顶点；而回写需要稀疏的
//!   顶点解（支撑集 = 实际用到的机制）。clarabel 也没有 crossover 选项。
//!
//! HiGHS 一个后端同时覆盖这两点：自带 presolve 与均衡缩放，单纯形直接给精确
//! 顶点解，必要时用内点法并在之后做 crossover。于是阈值扫描、提纯、基识别、
//! Ruiz 缩放全部删除。参数见 <https://ergo-code.github.io/HiGHS/dev/options/definitions/>。

use std::time::Instant;

use crate::concept::AIndexMap;
use good_lp::{
    Constraint, Expression, IntoAffineExpression, ProblemVariables, ResolutionError, Solution,
    SolverModel, Variable, VariableDefinition, solvers::highs::highs,
};

/// 一次求解的记录（随结果返回给上层）。
///
/// 应用没有安装 logger（`log` 宏是空操作），所以关键决策要随结果留痕。
/// HiGHS 单后端不做剪枝，这里只留「支撑集大小 + 原问题校验残差」。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SolveReport {
    /// 参与求解的变量数。
    pub variables_before: usize,
    /// 解里非零的变量数（顶点解下就是支撑集大小）。
    pub variables_after: usize,
    /// 用**原始 LP**评估这个解的最大相对约束违反量。
    pub primal_violation: f64,
}

/// 求解结果（原问题空间）。
pub struct LpSolution {
    values: AIndexMap<Variable, f64>,
    /// 上层按约束下标读原始对偶；HiGHS 不做外部缩放，恒为 1.0。
    pub dual_scales: Vec<f64>,
    pub cost: f64,
    /// 与 `dual_scales` 同理，恒为 1.0（保留给上层的取值还原路径）。
    pub global_scale: f64,
    /// 本次求解的记录（见 [`SolveReport`]）。
    pub report: SolveReport,
}

impl LpSolution {
    /// 变量在原问题空间的取值；没有记录（变量不存在）的为 0。
    pub fn value(&self, var: Variable) -> f64 {
        self.values.get(&var).copied().unwrap_or(0.0)
    }

    /// HiGHS 直接在原问题上求解，没有 Ruiz 缩放，恒为 1.0。
    pub fn prim_scale(&self, _var: Variable) -> f64 {
        1.0
    }
}

/// 解析后的一行约束：sum(coeff * x) {==,<=} rhs（rhs = -表达式常数项）。
struct ParsedRow {
    is_equality: bool,
    rhs: f64,
    /// (变量下标, 系数)
    terms: Vec<(usize, f64)>,
}

/// 由解析后的 LP 构建出的 good_lp 问题。
struct BuiltProblem {
    variables: ProblemVariables,
    objective: Expression,
    constraints: Vec<Constraint>,
    /// 出现「不含任何变量、又不可能满足」的约束（例如目标物品根本不在任何流里，
    /// 目标约束退化成 0 == 1）→ 原问题不可行，不必交给后端。
    trivially_infeasible: bool,
}

/// 把解析后的 `objective/rows` 重建成一个 good_lp 问题。
fn build_problem(
    defs: &[VariableDefinition],
    objective: &[(usize, f64)],
    rows: &[ParsedRow],
) -> BuiltProblem {
    let mut variables = ProblemVariables::new();
    let vars_in_order: Vec<Variable> = variables.add_all(defs.iter().cloned());
    let fold = |terms: &[(usize, f64)]| -> Expression {
        terms
            .iter()
            .map(|(index, coeff)| vars_in_order[*index] * *coeff)
            .fold(Expression::from(0.0), |acc, term| acc + term)
    };
    let objective_expr = fold(objective);
    let mut constraints = Vec::new();
    let mut trivially_infeasible = false;
    for row in rows {
        if row.terms.is_empty() {
            if (row.is_equality && row.rhs.abs() > 1e-12) || (!row.is_equality && row.rhs < 0.0) {
                trivially_infeasible = true;
            }
            continue;
        }
        let expr = fold(&row.terms);
        let constraint = if row.is_equality {
            expr.eq(row.rhs)
        } else {
            expr.leq(row.rhs)
        };
        constraints.push(constraint);
    }
    BuiltProblem {
        variables,
        objective: objective_expr,
        constraints,
        trivially_infeasible,
    }
}

/// 接受一个解的门槛：原问题最大相对约束违反量不超过它。
const ACCEPT_VIOLATION: f64 = 1e-6;
/// 绝对可行性余量：相对违反量之外再给一个绝对下限（见 [`evaluate_solution`]）。
const ABSOLUTE_VIOLATION: f64 = 1e-8;

/// 用**原始 LP**评估一个解：返回 (最大相对约束违反量, 目标值)。
///
/// 相对量按该行的总量级归一：`max(|rhs|, Σ|系数×取值|)`。再叠加一个绝对下限：
/// 没有它，`0 = 0` 这种**近乎空行**（rhs = 0、取值也只有 ~1e-12）会把残差按
/// 自身量级归一成「相对违反 1.0」，把本来正确的解误杀。
fn evaluate_solution(
    objective: &[(usize, f64)],
    rows: &[ParsedRow],
    values: &[f64],
) -> (f64, f64) {
    let mut max_violation = 0.0f64;
    for row in rows {
        let lhs: f64 = row
            .terms
            .iter()
            .map(|(index, coeff)| coeff * values[*index])
            .sum();
        let raw = if row.is_equality {
            (lhs - row.rhs).abs()
        } else {
            (lhs - row.rhs).max(0.0)
        };
        let magnitude: f64 = row
            .terms
            .iter()
            .map(|(index, coeff)| (coeff * values[*index]).abs())
            .sum();
        let row_scale = magnitude.max(row.rhs.abs());
        if raw <= ACCEPT_VIOLATION * row_scale + ABSOLUTE_VIOLATION {
            continue;
        }
        let denominator = if row_scale > 0.0 { row_scale } else { 1.0 };
        max_violation = max_violation.max(raw / denominator);
    }
    let objective_value: f64 = objective
        .iter()
        .map(|(index, coeff)| coeff * values[*index])
        .sum();
    (max_violation, objective_value)
}

/// 组装最终结果。
fn assemble(
    values: Vec<f64>,
    dual_scales: Vec<f64>,
    cost: f64,
    report: SolveReport,
    orig_vars: &[Variable],
) -> LpSolution {
    let values_map: AIndexMap<Variable, f64> = orig_vars
        .iter()
        .zip(values.iter())
        .map(|(var, value)| (*var, *value))
        .collect();
    LpSolution {
        values: values_map,
        dual_scales,
        cost,
        global_scale: 1.0,
        report,
    }
}

/// 求解一个线性规划：**HiGHS 一个后端**，直接给顶点解。
///
/// 失败处理：HiGHS 报 Infeasible/Unbounded 原样透出；解没过原问题校验则明确报
/// 「不能视为求解完成」，而不是返回一个不满足原约束的「成功」。
pub fn solve_lp(
    minimise: Expression,
    constraints: Vec<Constraint>,
    variables: ProblemVariables,
) -> Result<LpSolution, ResolutionError> {
    let defs: Vec<VariableDefinition> = variables
        .iter_variables_with_def()
        .map(|(_, def)| def.clone())
        .collect();
    let orig_vars: Vec<Variable> = variables
        .iter_variables_with_def()
        .map(|(var, _)| var)
        .collect();
    // good_lp 的 Variable::index() 是 crate 私有，这里按 iter 顺序自己建映射。
    let index_of: AIndexMap<Variable, usize> = variables
        .iter_variables_with_def()
        .enumerate()
        .map(|(index, (var, _))| (var, index))
        .collect();
    let objective: Vec<(usize, f64)> = minimise
        .clone()
        .linear_coefficients()
        .filter_map(|(var, coeff)| index_of.get(&var).map(|&index| (index, coeff)))
        .collect();
    let rows: Vec<ParsedRow> = constraints
        .iter()
        .map(|constraint| ParsedRow {
            is_equality: constraint.is_equality(),
            rhs: -constraint.expression().constant(),
            terms: constraint
                .expression()
                .linear_coefficients()
                .filter_map(|(var, coeff)| index_of.get(&var).map(|&index| (index, coeff)))
                .collect(),
        })
        .collect();

    let full = build_problem(&defs, &objective, &rows);
    if full.trivially_infeasible {
        return Err(ResolutionError::Infeasible);
    }
    if defs.is_empty() {
        return Ok(assemble(
            Vec::new(),
            vec![1.0; rows.len()],
            0.0,
            SolveReport::default(),
            &orig_vars,
        ));
    }

    let instant = Instant::now();
    let solution = full
        .variables
        .minimise(full.objective)
        .using(highs)
        // 默认 output_flag = true 会把 HiGHS 日志打到控制台，关掉。
        .set_option("output_flag", false)
        .with_all(full.constraints)
        .solve()?;
    let values: Vec<f64> = orig_vars.iter().map(|var| solution.value(*var)).collect();
    if values.iter().any(|value| !value.is_finite()) {
        return Err(ResolutionError::Other("HiGHS 返回非有限解"));
    }
    // **原问题校验**：HiGHS 有 presolve + postsolve，解未必在原空间完全满足约束。
    let (violation, cost) = evaluate_solution(&objective, &rows, &values);
    if violation > ACCEPT_VIOLATION {
        return Err(ResolutionError::Str(format!(
            "HiGHS 解未通过原问题校验（最大相对违反 {violation:.3e}）：不能视为求解完成"
        )));
    }
    let variables_used = values.iter().filter(|value| value.abs() > 0.0).count();
    log::info!(
        "HiGHS 求解完成：{} 个变量，支撑集 {} 个，耗时 {:.2?}",
        defs.len(),
        variables_used,
        instant.elapsed()
    );
    Ok(assemble(
        values,
        vec![1.0; rows.len()],
        cost,
        SolveReport {
            variables_before: defs.len(),
            variables_after: variables_used,
            primal_violation: violation,
        },
        &orig_vars,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use good_lp::*;

    /// 退化最优面（50 个等效列，和为 1）应被压到**一个**顶点列：单纯形直接给
    /// 基本解（支撑集 = 1），不再需要「剪枝比例扫描 + 第二后端」。
    #[test]
    fn degenerate_optimal_face_collapses_to_one_column() {
        let mut vars = ProblemVariables::new();
        let xs: Vec<Variable> = (0..50).map(|_| vars.add(variable().min(0.0))).collect();
        let mut sum = Expression::from(0.0);
        for &x in &xs {
            sum += x;
        }
        let solution = solve_lp(sum.clone(), vec![sum.eq(1.0)], vars).expect("应可解");
        let support: Vec<f64> = xs
            .iter()
            .map(|&x| solution.value(x))
            .filter(|value| value.abs() > 0.0)
            .collect();
        assert_eq!(support.len(), 1, "退化最优面应只留一个顶点列：{support:?}");
        assert!((support[0] - 1.0).abs() < 1e-6, "该列取值应为 1：{support:?}");
        assert_eq!(solution.report.variables_after, 1);
    }

    /// 小但必要的变量不能被「取值大小」剪掉：单纯形解里它就是基变量。
    #[test]
    fn keeps_small_but_essential_variable() {
        let mut vars = ProblemVariables::new();
        let big = vars.add(variable().min(0.0));
        let small = vars.add(variable().min(0.0));
        let constraints = vec![
            big.into_expression().eq(1.0e6),
            small.into_expression().eq(1.0e-3),
        ];
        let solution = solve_lp(big + small, constraints, vars).expect("应可解");
        assert!(
            (solution.value(big) - 1.0e6).abs() < 1e-3,
            "big：{}",
            solution.value(big)
        );
        assert!(
            (solution.value(small) - 1.0e-3).abs() < 1e-9,
            "small（必要的小变量）被剪掉了：{}",
            solution.value(small)
        );
    }

    /// 跨二十个数量级、列高度相似的大 LP：旧实现里 microlp 报 Singular matrix
    /// 的形态。构造 2000 个近乎相同的列，每个强制取一个很小的量，总和固定。
    #[test]
    fn large_similar_columns_stay_solvable() {
        let mut vars = ProblemVariables::new();
        let xs: Vec<Variable> = (0..2000).map(|_| vars.add(variable().min(0.0))).collect();
        let mut sum = Expression::from(0.0);
        for (i, &x) in xs.iter().enumerate() {
            let scale = 1.0 + (i as f64) * 1e-9;
            sum += scale * x;
        }
        let solution = solve_lp(sum.clone(), vec![sum.eq(1.0)], vars).expect("应可解");
        let support = xs
            .iter()
            .filter(|&&x| solution.value(x).abs() > 0.0)
            .count();
        assert!(support >= 1, "至少要有一个列在用：{support}");
        assert!(solution.value(xs[0]) <= 1.0 + 1e-6);
    }
}
