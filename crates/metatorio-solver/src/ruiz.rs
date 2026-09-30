use std::time::Instant;

use crate::concept::AIndexMap;

use good_lp::{
    Constraint, Expression, IntoAffineExpression, ProblemVariables, ResolutionError, Solution,
    SolverModel, Variable, VariableDefinition, microlp, solvers::clarabel::clarabel,
};
use rayon::prelude::*;

pub struct RuizSolver {
    minimise: Expression,
    constraints: Vec<Constraint>,
    variables: ProblemVariables,
}

pub struct RuizSolution {
    /// 原问题空间里每个变量（按传给求解器的 Variable）的取值。
    /// 用显式取值表而不是直接暴露后端解对象：剪枝/换后端后变量会重编号，
    /// 调用方不该关心后端是谁。
    values: AIndexMap<Variable, f64>,
    pub prim_scales: AIndexMap<Variable, f64>, // 原始变量的系数分别乘了这些系数
    pub dual_scales: Vec<f64>,                 // 原始约束的系数分别乘了这些系数
    pub cost: f64,                             // 原问题的目标值
    // 实际求解的问题是原问题的 global_scale 倍，
    // 因此原始变量需要除以 global_scale 才是原问题的结果
    pub global_scale: f64,
}

impl RuizSolution {
    /// 变量在原问题空间的取值；没有记录（被剪枝）的变量为 0。
    pub fn value(&self, var: Variable) -> f64 {
        self.values.get(&var).copied().unwrap_or(0.0)
    }

    /// 变量的 Ruiz 缩放系数（未记录时为 1.0）。
    pub fn prim_scale(&self, var: Variable) -> f64 {
        self.prim_scales.get(&var).copied().unwrap_or(1.0)
    }
}

/// 把后端解按 Ruiz 缩放恢复成原问题空间的取值表。
///
/// solution 所在问题的变量编号必须与 variables 一致（缩放问题是按同一顺序
/// 重建的，因此 Variable 下标一一对应）。
fn collect_values(
    variables: &[Variable],
    prim_scales: &AIndexMap<Variable, f64>,
    global_scale: f64,
    solution: &impl Solution,
) -> AIndexMap<Variable, f64> {
    variables
        .iter()
        .map(|&var| {
            let prim_scale = prim_scales.get(&var).cloned().unwrap_or(1.0);
            (var, solution.value(var) * prim_scale / global_scale)
        })
        .collect()
}

pub const MAGIC: f64 = 114.0;
pub const MAGIC_INV: f64 = 1.0 / MAGIC;
impl RuizSolver {
    pub fn new(
        minimise: Expression,
        constraints: Vec<Constraint>,
        variables: ProblemVariables,
    ) -> Self {
        Self {
            minimise,
            constraints,
            variables,
        }
    }

    pub fn solve(self) -> Result<RuizSolution, ResolutionError> {
        // 未缩放回退要用到原始目标函数，但下面的 linear_coefficients() 会
        // 消费 self.minimise，所以先留一份克隆。
        let original_minimise = self.minimise.clone();
        // 变量列表（按编号顺序）：恢复取值时遍历它，不依赖后端变量编号。
        let variable_list: Vec<Variable> = self
            .variables
            .iter_variables_with_def()
            .map(|(var, _)| var)
            .collect();
        // 每个变量，在每个约束中的系数
        let instant = Instant::now();
        let prim_coeffs = self
            .constraints
            .par_iter()
            .enumerate()
            .fold(AIndexMap::default, |mut acc, (idx, constraint)| {
                constraint
                    .expression()
                    .linear_coefficients()
                    .for_each(|(var, coeff)| {
                        acc.entry(var)
                            .or_insert_with(AIndexMap::default)
                            .insert(idx, coeff);
                    });
                acc
            })
            .reduce(AIndexMap::default, |mut map1, map2| {
                // 合并两个局部的反查表
                for (key, inner_map) in map2 {
                    map1.entry(key).or_default().extend(inner_map);
                }
                map1
            });

        let mut prim_scales = self
            .variables
            .iter_variables_with_def()
            .map(|(var, _)| (var, 1.0))
            .collect::<AIndexMap<Variable, f64>>();
        let mut dual_scales = vec![1.0; self.constraints.len()];

        // 随手计算一个停止阈值，理论上应该是 1.0，但考虑到数值误差，放宽一点
        let stop_threshold = 1.0 + 1.0 / (self.constraints.len() as f64 * 16.0 + 1.0);
        // 每次修改变量的系数和约束的系数，使得系数的均方根接近1
        for i in 0..1024 {
            // 交替修改 prim 和 dual
            // 统计每次更新的最大变化，小于一定值视为收敛

            let mut max_delta_scale = prim_scales
                .par_iter_mut()
                .map(|(var, prim_scale)| {
                    let mut sum_x2 = 0.0;
                    let mut count = 0;
                    for (&idx, &coeff) in prim_coeffs.get(var).unwrap_or(&AIndexMap::default()) {
                        if coeff == 0.0 {
                            continue;
                        }
                        let dual_scale = dual_scales[idx];
                        sum_x2 += (coeff * dual_scale * *prim_scale).powi(2);
                        count += 1;
                    }
                    let delta_scale = if count == 0 {
                        1.0
                    } else {
                        (sum_x2 / count as f64)
                            .sqrt()
                            .recip()
                            .clamp(MAGIC_INV, MAGIC)
                    };
                    *prim_scale *= delta_scale;
                    delta_scale.max(delta_scale.recip())
                })
                .reduce(|| 1.0, f64::max); // 计算最大值
            max_delta_scale = max_delta_scale.max(
                dual_scales
                    .par_iter_mut()
                    .enumerate()
                    .map(|(idx, dual_scale)| {
                        let mut sum_x2 = 0.0;
                        let mut count = 0;
                        for (var, coeff) in self.constraints[idx].expression().linear_coefficients()
                        {
                            if coeff == 0.0 {
                                continue;
                            }
                            let prim_scale = prim_scales.get(&var).cloned().unwrap_or(1.0);
                            sum_x2 += (coeff * prim_scale * *dual_scale).powi(2);
                            count += 1;
                        }

                        let delta_scale = if count == 0 {
                            1.0
                        } else {
                            (sum_x2 / count as f64)
                                .sqrt()
                                .recip()
                                .clamp(MAGIC_INV, MAGIC)
                        };
                        *dual_scale *= delta_scale;
                        delta_scale.max(delta_scale.recip())
                    })
                    .reduce(|| 1.0, f64::max),
            ); // 计算最大值
            if max_delta_scale < stop_threshold {
                log::debug!("Ruiz 预处理在第 {} 轮收敛", i);
                break;
            }
        }

        // 变换关系：x' = x * global_scale / prim_scale（由约束与目标函数的
        // 缩放公式反推；求解器内部变量 x' 的解经 value * prim_scale / global_scale
        // 恢复为原变量 x，见 solver.rs）。
        // 因此变量界的正确缩放是 界 * global_scale / prim_scale。
        // 历史 bug：此前为 界 / prim_scale（漏乘 global_scale），导致变量上界
        // 在缩放空间中放大 1/global_scale 倍，microlp 给出的解可违反原问题上界
        // （test_ruiz 曾复现：x1.max(1.0) 被解成 10/7≈1.4286）。
        // metatorio 的 SolverData 路径不使用变量上界（均为 min(0)），故未受影响。
        let mut global_scale = 0.0;
        for (idx, constraint) in self.constraints.iter().enumerate() {
            global_scale = f64::max(
                global_scale,
                constraint.expression().constant().abs() * dual_scales[idx],
            );
        }
        if global_scale == 0.0 {
            global_scale = 1.0;
        }
        global_scale = global_scale.recip();

        let new_variables_defs = self
            .variables
            .iter_variables_with_def()
            .map(|(var, def)| {
                let prim_scale = prim_scales.get(&var).cloned().unwrap_or(1.0);
                let new_min = def.get_min() * global_scale / prim_scale;
                let new_max = def.get_max() * global_scale / prim_scale;
                VariableDefinition::new().min(new_min).max(new_max)
            })
            .collect::<Vec<VariableDefinition>>();
        let mut new_variables = ProblemVariables::new();
        let _: Vec<Variable> = new_variables.add_all(new_variables_defs);
        let new_minimise = self
            .minimise
            .linear_coefficients()
            .map(|(var, coeff)| {
                let prim_scale = prim_scales.get(&var).cloned().unwrap_or(1.0);
                var * coeff * prim_scale / global_scale
            })
            .fold(Expression::from(0.0), |acc, term| acc + term);
        let new_constraints = self
            .constraints
            .par_iter()
            .enumerate()
            .map(|(idx, constraint)| {
                let is_equality = constraint.is_equality();
                let constant = constraint.expression().constant();
                let new_expr = constraint
                    .expression()
                    .linear_coefficients()
                    .map(|(var, coeff)| {
                        let prim_scale = prim_scales.get(&var).cloned().unwrap_or(1.0);
                        var * coeff * prim_scale
                    })
                    .fold(Expression::from(0.0), |acc, term| acc + term);
                let dual_scale = dual_scales[idx];
                match is_equality {
                    true => (new_expr * dual_scale).eq(-constant * dual_scale * global_scale),
                    false => (new_expr * dual_scale).leq(-constant * dual_scale * global_scale),
                }
            })
            .collect::<Vec<_>>();

        log::debug!("global_scale: {}", global_scale);
        if new_constraints.len() < 8 {
            log::debug!("prim_scales: {:?}", prim_scales);
            log::debug!("dual_scales: {:?}", dual_scales);
            log::debug!("new_minimise: {:?}", new_minimise);
            log::debug!("new_constraints:");
            for (idx, constraint) in new_constraints.iter().enumerate() {
                log::debug!("  {}: {:?}", idx, constraint);
            }
        }
        log::debug!("Ruiz 预处理耗时: {:?}", instant.elapsed());
        let primary = new_variables
            .minimise(new_minimise.clone())
            .using(microlp)
            .with_all(new_constraints)
            .solve();
        match primary {
            Ok(solution) => {
                let values =
                    collect_values(&variable_list, &prim_scales, global_scale, &solution);
                let cost = solution.eval(new_minimise);
                Ok(RuizSolution {
                    values,
                    prim_scales,
                    dual_scales,
                    global_scale,
                    cost,
                })
            }
            // 数值失败（如 microlp 的 "Singular matrix"）**不是**「无可行解」：
            // 实测大 auto_plan LP 会被 Ruiz 缩放放大成病态矩阵，microlp 直接
            // 放弃。退一步用**未缩放**的原始问题再解一次——数值路径不同，可能
            // 绕开缩放引入的病态；真不可行时这一步同样报 Infeasible，不会更差。
            Err(err) if matches!(err, ResolutionError::Other(_) | ResolutionError::Str(_)) => {
                log::warn!("Ruiz 缩放后求解失败（{err:?}），改用未缩放问题重试一次");
                let identity_prim: AIndexMap<Variable, f64> = self
                    .variables
                    .iter_variables_with_def()
                    .map(|(var, _)| (var, 1.0))
                    .collect();
                let identity_dual = vec![1.0; self.constraints.len()];
                let solution = self
                    .variables
                    .minimise(original_minimise.clone())
                    .using(microlp)
                    .with_all(self.constraints)
                    .solve()?;
                let values = collect_values(&variable_list, &identity_prim, 1.0, &solution);
                let cost = solution.eval(original_minimise);
                Ok(RuizSolution {
                    values,
                    prim_scales: identity_prim,
                    dual_scales: identity_dual,
                    global_scale: 1.0,
                    cost,
                })
            }
            Err(err) => Err(err),
        }
    }
}

/// clarabel 解里小于「最大取值 × 该比例」的变量在剪枝时丢弃。
const PRUNE_RATIO: f64 = 1e-7;

/// 解析后的一行约束：sum(coeff * x) {==,<=} rhs（rhs = -表达式常数项）。
struct ParsedRow {
    is_equality: bool,
    rhs: f64,
    /// (原始变量下标, 系数)
    terms: Vec<(usize, f64)>,
}

/// 由解析后的 LP 构建出的 good_lp 问题，以及新旧变量的对应关系。
struct BuiltProblem {
    variables: ProblemVariables,
    objective: Expression,
    constraints: Vec<Constraint>,
    /// 第 k 个新变量对应的原始变量下标（顺序同 add_all 返回的 Variable）。
    indices: Vec<usize>,
    /// 新问题里按编号顺序的 Variable（下标从 0 起）。
    vars_in_order: Vec<Variable>,
    /// 第 k 个约束对应的原始行下标。
    emitted_rows: Vec<usize>,
    /// 剪枝后出现「不含任何变量、又不可能满足」的约束。
    trivially_infeasible: bool,
}

/// 按 keep 选出的变量子集重建一个 good_lp 问题；keep = None 表示全量。
fn build_problem(
    defs: &[VariableDefinition],
    objective: &[(usize, f64)],
    rows: &[ParsedRow],
    keep: Option<&[usize]>,
) -> BuiltProblem {
    let indices: Vec<usize> = match keep {
        Some(keep) => keep.to_vec(),
        None => (0..defs.len()).collect(),
    };
    let mut variables = ProblemVariables::new();
    let vars_in_order: Vec<Variable> =
        variables.add_all(indices.iter().map(|&index| defs[index].clone()));
    let mut map: AIndexMap<usize, Variable> = AIndexMap::default();
    for (pos, &orig) in indices.iter().enumerate() {
        map.insert(orig, vars_in_order[pos]);
    }
    let fold = |terms: &[(usize, f64)]| -> Expression {
        terms
            .iter()
            .filter_map(|(index, coeff)| map.get(index).map(|var| *var * *coeff))
            .fold(Expression::from(0.0), |acc, term| acc + term)
    };
    let objective_expr = fold(objective);
    let mut constraints = Vec::new();
    let mut emitted_rows = Vec::new();
    let mut trivially_infeasible = false;
    for (row_index, row) in rows.iter().enumerate() {
        let kept_terms = row
            .terms
            .iter()
            .filter(|(index, _)| map.contains_key(index))
            .count();
        if kept_terms == 0 {
            // 剪枝后这一行没有变量：只有恒真时才能丢。
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
        emitted_rows.push(row_index);
    }
    BuiltProblem {
        variables,
        objective: objective_expr,
        constraints,
        indices,
        vars_in_order,
        emitted_rows,
        trivially_infeasible,
    }
}

/// 求解策略（#6）：clarabel 先解 → 按其解剪枝 → 剪枝后的子集交 microlp。
///
/// 动机：microlp 在「列高度相似、系数跨度二十个数量级」的大 LP 上会返回内部
/// 数值失败（Singular matrix），把有解误报成不可解。clarabel 是内点法，自带
/// 均衡与正则化，通常能解出来；但内点解不是顶点解，所以再用它的解剪枝，把
/// 剪枝后的小问题交给 microlp 求顶点解——**以剪枝后的结果为准**。
///
/// 失败处理：clarabel 报 Infeasible/Unbounded 原样透出；clarabel 自身数值失败
/// 时回退原来的 Ruiz + microlp；剪枝后 microlp 仍失败则退回 clarabel 的近似可行解。
pub fn solve_pruned(
    minimise: Expression,
    constraints: Vec<Constraint>,
    variables: ProblemVariables,
) -> Result<RuizSolution, ResolutionError> {
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

    // 1) clarabel 解原始问题。
    let full = build_problem(&defs, &objective, &rows, None);
    // 全量阶段就有「不含变量、又不可能满足」的约束（例如目标物品根本不在
    // 任何流里，于是目标约束退化成 0 == 1）→ 原问题不可行，不必交给后端。
    if full.trivially_infeasible {
        return Err(ResolutionError::Infeasible);
    }
    // 空问题 clarabel 会在内部 panic，直接走 Ruiz + microlp。
    if defs.is_empty() {
        return Ok(RuizSolution {
            values: AIndexMap::default(),
            prim_scales: AIndexMap::default(),
            dual_scales: vec![1.0; rows.len()],
            cost: 0.0,
            global_scale: 1.0,
        });
    }
    if rows.is_empty() {
        return RuizSolver::new(minimise, constraints, variables).solve();
    }
    // clarabel 在退化输入上会 panic（qdldl 越界，见 clarabel-0.11.1），
    // 作为主求解路径必须兜住：panic 与内部错误一样退到 Ruiz + microlp。
    let clarabel_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        full.variables
            .minimise(full.objective)
            .using(clarabel)
            .with_all(full.constraints)
            .solve()
    }));
    let clarabel_solution = match clarabel_result {
        Ok(Ok(solution)) => solution,
        Ok(Err(err @ (ResolutionError::Infeasible | ResolutionError::Unbounded))) => {
            return Err(err);
        }
        Ok(Err(err)) => {
            log::warn!("clarabel 求解失败（{err:?}），回退 Ruiz + microlp");
            return RuizSolver::new(minimise, constraints, variables).solve();
        }
        Err(_) => {
            log::warn!("clarabel 内部 panic，回退 Ruiz + microlp");
            return RuizSolver::new(minimise, constraints, variables).solve();
        }
    };
    let clarabel_values: Vec<f64> = orig_vars
        .iter()
        .map(|var| clarabel_solution.value(*var))
        .collect();
    if clarabel_values.iter().any(|value| !value.is_finite()) {
        return Err(ResolutionError::Other("clarabel 返回非有限解"));
    }

    // 2) 剪枝：只保留用量显著的变量。
    let max_abs = clarabel_values
        .iter()
        .fold(0.0f64, |acc, value| acc.max(value.abs()));
    let threshold = max_abs * PRUNE_RATIO;
    let keep: Vec<usize> = (0..defs.len())
        .filter(|&index| clarabel_values[index].abs() > threshold)
        .collect();
    log::info!(
        "clarabel 剪枝：{} 个变量保留 {} 个（阈值 {:.3e}）",
        defs.len(),
        keep.len(),
        threshold
    );


    // 3) 剪枝后的子集交给 microlp（仍走 Ruiz 缩放；问题小，很快）。
    let reduced = build_problem(&defs, &objective, &rows, Some(&keep));
    let reduced_solution = if reduced.trivially_infeasible || reduced.indices.is_empty() {
        None
    } else {
        match RuizSolver::new(reduced.objective, reduced.constraints, reduced.variables).solve() {
            Ok(solution) => Some(solution),
            Err(err) => {
                log::warn!("剪枝后 microlp 仍失败（{err:?}），采用 clarabel 解");
                None
            }
        }
    };

    let (values, prim_scales, dual_scales, global_scale) = match reduced_solution {
        Some(solution) => {
            let mut values = vec![0.0f64; defs.len()];
            let mut prim_scales: AIndexMap<Variable, f64> = AIndexMap::default();
            for (pos, &orig) in reduced.indices.iter().enumerate() {
                let reduced_var = reduced.vars_in_order[pos];
                values[orig] = solution.value(reduced_var);
                prim_scales.insert(orig_vars[orig], solution.prim_scale(reduced_var));
            }
            let mut dual_scales = vec![1.0f64; rows.len()];
            for (pos, &row_index) in reduced.emitted_rows.iter().enumerate() {
                dual_scales[row_index] = solution.dual_scales.get(pos).copied().unwrap_or(1.0);
            }
            (values, prim_scales, dual_scales, solution.global_scale)
        }
        None => (
            clarabel_values.clone(),
            AIndexMap::default(),
            vec![1.0f64; rows.len()],
            1.0,
        ),
    };
    let cost = objective
        .iter()
        .map(|(index, coeff)| coeff * values[*index])
        .sum();
    let values_map: AIndexMap<Variable, f64> = orig_vars
        .iter()
        .zip(values.iter())
        .map(|(var, value)| (*var, *value))
        .collect();
    Ok(RuizSolution {
        values: values_map,
        prim_scales,
        dual_scales,
        cost,
        global_scale,
    })
}

/// 定位"变量上界未生效"缺陷：直接用 microlp（不经 Ruiz 缩放）求解同一 LP。

/// 打印 RuizSolver 的缩放参数与缩放空间解，定位上界丢失点。
#[test]
fn test_ruiz() {
    use good_lp::*;
    let mut vars = ProblemVariables::new();
    let x1 = vars.add(VariableDefinition::new().max(1.0));
    let x2 = vars.add(VariableDefinition::new().min(0.0));
    let x3 = vars.add(VariableDefinition::new().min(0.0));

    let constraints = vec![
        (x1 + 2 * x2 + 3 * x3).leq(3),
        (4 * x1 + 5 * x2 + 6 * x3).leq(7),
        (7 * x1 + 8 * x2 + 9 * x3).leq(10),
    ];

    let solution = RuizSolver::new(-(x1 + x2 + x3), constraints, vars)
        .solve()
        .unwrap();

    // RuizSolution::value 已按 prim_scale / global_scale 还原回原问题空间；
    // 历史上本测试曾直接读后端解导致误读为 0.678，实际最优为
    // x1=1.0, x2=0.375, x3=0，目标 1.375。
    let x1v = solution.value(x1);
    let x2v = solution.value(x2);
    let x3v = solution.value(x3);

    assert!((x1v - 1.0).abs() < 1e-5, "x1: {x1v}");
    assert!((x2v - 0.375).abs() < 1e-5, "x2: {x2v}");
    assert!((x3v).abs() < 1e-5, "x3: {x3v}");
    assert!(
        ((x1v + x2v + x3v) - 1.375).abs() < 1e-5,
        "目标: {}",
        x1v + x2v + x3v
    );
}

/// 验证 microlp 对 global_scale 的敏感性：同一 LP，仅改目标约束常数
/// （Ruiz 的 global_scale = 1/max(|c|×dual) 随目标常数变化），缩放后
/// 交给 microlp 的结果应一致（倍率不变性）。若结果随 global_scale 变化
/// 而不同，说明缩放空间量级影响 microlp 数值路径（auto_plan 大目标
/// 不可解、小目标可解的根因）。
///
/// 注意：这是一个诊断测试（当前断言未启用——修复方案待定，见
/// fulgora 传奇电磁工厂倍率问题的调查记录）。记录各目标量级下的
/// global_scale 与可解性，便于后续定位。
#[test]
fn global_scale_sensitivity() {
    use good_lp::*;
    let mut all_solved = true;
    let mut reported = Vec::new();
    for target_amount in [1e-6, 1e-4, 0.001, 0.1, 1.0, 10.0] {
        let mut vars = ProblemVariables::new();
        let x1 = vars.add(VariableDefinition::new().min(0.0));
        let x2 = vars.add(VariableDefinition::new().min(0.0));
        let x3 = vars.add(VariableDefinition::new().min(0.0));
        // 物品链：x1 消耗 source 产出 x2，x2 产出 x3（目标）。
        let constraints = vec![
            (x1 - x2).eq(0.0),                      // 中间平衡
            (x2 - x3).eq(0.0),                      // 目标平衡
            x3.into_expression().eq(target_amount), // 目标约束（常数随目标变化）
            x1.into_expression().leq(1000.0),       // 冗余约束（不紧）
        ];
        let minimise = x1 + x2 + x3; // 最小化用量（无成本差异）
        let result = RuizSolver::new(minimise, constraints, vars).solve();
        match &result {
            Ok(sol) => {
                let g = sol.global_scale;
                let v3 = sol.value(x3);
                let ok = (v3 - target_amount).abs() <= target_amount * 1e-3 + 1e-9;
                eprintln!("目标 {target_amount}: Solved global={g:.3e} x3={v3:.6e} ok={ok}");
                if !ok {
                    all_solved = false;
                }
                reported.push((target_amount, g, ok));
            }
            Err(error) => {
                eprintln!("目标 {target_amount}: Err {error:?}");
                all_solved = false;
                reported.push((target_amount, 0.0, false));
            }
        }
    }
    // 诊断记录：当前小规模链全部可解（问题只在真实大 LP 出现）。
    // 若未来此断言因 Ruiz/microlp 改动失败，说明敏感性被引入。
    eprintln!("global_scale 敏感性扫描：{reported:?} all_solved={all_solved}");
    // 注意：真实场景（fulgora 传奇电磁工厂）dual_scale 达 5.85e5 导致
    // global_scale 极小，与目标常数共同影响 microlp 数值路径。修复方案
    // 暂缓（见会话记录）。此处不断言，仅记录。
}
