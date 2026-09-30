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

/// 一次求解的剪枝/后端记录（随结果返回给上层）。
///
/// 求解器里有多处**启发式**剪枝。极端 mod 下如果结果看起来「少了一条关键
/// 机制」，必须能判断它是被剪枝丢掉的，还是根本没有候选 / 真的不可解——
/// 所以这些决策要随结果留痕，而不是只写日志（应用未安装 logger，`log`
/// 宏是空操作，等于没记）。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SolveReport {
    /// 结果是否退回了 clarabel 的**稠密内点解**——这种解带 ~1e-8 的数值尾值，
    /// 上层必须用相对阈值过滤，否则会把尾值当成「在用」。
    pub dense_fallback: bool,
    /// 参与剪枝判定的变量数。
    pub variables_before: usize,
    /// 实际参与求解的变量数（未剪枝时与 before 相等）。
    pub variables_after: usize,
    /// 剪枝阈值（clarabel 解最大取值 × PRUNE_RATIO）；未剪枝为 0。
    pub prune_threshold: f64,
    /// 用**原始 LP**（未剪枝的全部约束）评估这个解的最大相对违反量。
    /// 剪枝是把变量钉成 0，子问题的解**未必**满足原问题约束。
    pub primal_violation: f64,
    /// 与 clarabel 参考目标值的相对差：> 0 说明比参考解更差（剪枝丢了更优解）。
    pub objective_gap: f64,
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
    /// 本次求解的剪枝/后端记录（见 [`SolveReport`]）。
    pub report: SolveReport,
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
                    report: SolveReport {
                        variables_before: variable_list.len(),
                        variables_after: variable_list.len(),
                        ..SolveReport::default()
                    },
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
                    report: SolveReport {
                        variables_before: variable_list.len(),
                        variables_after: variable_list.len(),
                        ..SolveReport::default()
                    },
                })
            }
            Err(err) => Err(err),
        }
    }
}

/// clarabel 解里小于「最大取值 × 该比例」的变量在剪枝时丢弃。
/// 剪枝比例：**从紧到松**依次尝试，取第一个「剪枝后子问题能解出来」的档。
///
/// 单一阈值两边都会踩坑（实测原版 nauvis / `transport-belt:legendary`）：
/// - 太紧（1e-7 / 1e-9）：子问题丢掉配平必需的变量 → 解不出来 → 退回全量
///   microlp → 全量在病态输入上给出的解自己不可复现 → 回写后文档无解；
/// - 太松（0 = 全保留）：等于没剪 → microlp 又回到病态 → 同样失败；
/// - 中间（1e-11）：保留 474 个变量，microlp 解出干净顶点解 → 回写 39 条，
///   文档可解。
///
/// 所以这里的顺序必须**由紧到松**：先要最干净的解，解不出来再放宽。
const PRUNE_RATIOS: [f64; 4] = [1e-7, 1e-9, 1e-11, 0.0];

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
            report: SolveReport::default(),
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
    // 参考目标值：clarabel 给出的可行点。最小化时它是原问题最优值的**上界**——
    // 任何剪枝解只要不比它差，就说明剪枝没有丢掉更优解。
    let clarabel_objective: f64 = objective
        .iter()
        .map(|(index, coeff)| coeff * clarabel_values[*index])
        .sum();

    // 2) 剪枝：按「比例从紧到松」依次尝试，取第一个**子问题能解出来**的档。
    //
    // 为什么是一串而不是一个值（实测原版 nauvis / transport-belt:legendary）：
    //   1e-7  → 保留太少，子问题丢变量解不出来 → 退回全量 microlp → 回写后无解
    //   1e-9  → 同上
    //   1e-11 → 保留 474 个变量，microlp 解出干净顶点解 → 回写 39 条，文档可解 ✔
    //   0     → 全保留 = 全量，又回到病态 → 回写后无解
    // 也就是说：剪枝是**为了数值条件数**，但不能把配平必需的变量剪掉；
    // 单点阈值两边都会踩坑，所以按序列试到成功为止。
    // 变量「影响量」= |取值| × 该变量在所有约束里的最大系数。
    //
    // 不能拿 |取值| 的全局最大值当阈值：LP 里混着不同量纲（物品数、焦耳、温度…），
    // 焦耳量级的燃料流会把阈值抬到 1e-4——对物品流来说很大，于是一批对**小量纲行**
    // 至关重要的变量被剪掉：解虽然目标最优，却违反了原问题约束（实测最大相对违反
    // 2.9e1）。用系数除掉量纲才是可比的。
    let mut column_max = vec![0.0f64; defs.len()];
    for row in &rows {
        for (index, coeff) in &row.terms {
            let slot = &mut column_max[*index];
            *slot = slot.max(coeff.abs());
        }
    }
    let impact: Vec<f64> = clarabel_values
        .iter()
        .enumerate()
        .map(|(index, value)| value.abs() * column_max[index])
        .collect();
    let max_impact = impact.iter().fold(0.0f64, |acc, value| acc.max(*value));
    // 诊断用：设了 METATORIO_PRUNE_RATIO 就只试这一个比例。
    let ratios: Vec<f64> = match std::env::var("METATORIO_PRUNE_RATIO")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
    {
        Some(ratio) => vec![ratio],
        None => PRUNE_RATIOS.to_vec(),
    };
    let mut last_keep: Vec<usize> = Vec::new();
    for &ratio in &ratios {
        let threshold = max_impact * ratio;
        let keep: Vec<usize> = (0..defs.len())
            .filter(|&index| impact[index] > threshold)
            .collect();
        log::info!(
            "clarabel 剪枝：{} 个变量保留 {} 个（比例 {:.1e}，阈值 {:.3e}）",
            defs.len(),
            keep.len(),
            ratio,
            threshold
        );
        if keep.is_empty() {
            continue;
        }
        // 退路（clarabel 掩码）用**最松**的那一档：保留得最全，最不容易漏东西。
        last_keep = keep.clone();
        let reduced = build_problem(&defs, &objective, &rows, Some(&keep));
        if reduced.trivially_infeasible {
            continue;
        }
        let solution = match RuizSolver::new(
            reduced.objective,
            reduced.constraints,
            reduced.variables,
        )
        .solve()
        {
            Ok(solution) => solution,
            Err(err) => {
                log::warn!("剪枝（比例 {ratio:.1e}，保留 {}）求解失败：{err:?}", keep.len());
                continue;
            }
        };
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
        // **原问题校验**：剪枝后的解必须同时满足原问题的约束、且不比参考解差。
        let (violation, objective_value) = evaluate_solution(&objective, &rows, &values);
        let gap = (objective_value - clarabel_objective) / clarabel_objective.abs().max(1.0);
        if violation <= ACCEPT_VIOLATION && gap <= ACCEPT_OBJECTIVE_GAP {
            return Ok(assemble(
                values,
                prim_scales,
                dual_scales,
                solution.global_scale,
                SolveReport {
                    dense_fallback: false,
                    variables_before: defs.len(),
                    variables_after: keep.len(),
                    prune_threshold: threshold,
                    primal_violation: violation,
                    objective_gap: gap,
                },
                &objective,
                &orig_vars,
            ));
        }
        // 剪枝改变了解（违反原约束，或目标变差）→ 换更松的一档，绝不当成求解完成。
        log::warn!(
            "剪枝（比例 {ratio:.1e}，保留 {}）未过原问题校验：违反 {violation:.3e}，目标差 {gap:.3e}",
            keep.len()
        );
    }

    // 3) 所有剪枝档都没过校验 → 整问题 microlp（同为顶点解）。
    if let Ok(solution) = RuizSolver::new(minimise, constraints, variables).solve() {
        let values: Vec<f64> = orig_vars.iter().map(|var| solution.value(*var)).collect();
        let prim_scales: AIndexMap<Variable, f64> = orig_vars
            .iter()
            .map(|var| (*var, solution.prim_scale(*var)))
            .collect();
        let (violation, objective_value) = evaluate_solution(&objective, &rows, &values);
        let gap = (objective_value - clarabel_objective) / clarabel_objective.abs().max(1.0);
        if violation <= ACCEPT_VIOLATION && gap <= ACCEPT_OBJECTIVE_GAP {
            return Ok(assemble(
                values,
                prim_scales,
                solution.dual_scales.clone(),
                solution.global_scale,
                SolveReport {
                    dense_fallback: false,
                    variables_before: defs.len(),
                    variables_after: defs.len(),
                    prune_threshold: max_impact * ratios.last().copied().unwrap_or(PRUNE_RATIOS[0]),
                    primal_violation: violation,
                    objective_gap: gap,
                },
                &objective,
                &orig_vars,
            ));
        }
        log::warn!("整问题 microlp 未过原问题校验：违反 {violation:.3e}，目标差 {gap:.3e}");
    }

    // 4) 退回 clarabel 的解。**必须按剪枝掩码**：clarabel 是内点法，解是稠密的
    //    （没被用到的变量也会拿到 ~1e-8 的尾值），直接拿来用会让自动规划回写
    //    一堆接近 0 的机制（实测：140 条里 47 条 |rate| < 1e-6）。
    let mut values = vec![0.0f64; defs.len()];
    for &index in &last_keep {
        values[index] = clarabel_values[index];
    }
    let (violation, objective_value) = evaluate_solution(&objective, &rows, &values);
    let gap = (objective_value - clarabel_objective) / clarabel_objective.abs().max(1.0);
    if violation <= ACCEPT_VIOLATION && gap <= ACCEPT_OBJECTIVE_GAP {
        return Ok(assemble(
            values,
            AIndexMap::default(),
            vec![1.0f64; rows.len()],
            1.0,
            SolveReport {
                dense_fallback: true,
                variables_before: defs.len(),
                variables_after: last_keep.len(),
                prune_threshold: max_impact * ratios.last().copied().unwrap_or(PRUNE_RATIOS[0]),
                primal_violation: violation,
                objective_gap: gap,
            },
            &objective,
            &orig_vars,
        ));
    }
    // 一条都没过校验 → **不算求解完成**（剪枝可能改变了解集）。
    Err(ResolutionError::Str(format!(
        "剪枝后的解均未通过原问题校验（最后一次：最大相对违反 {violation:.3e}，目标相对差 {gap:.3e}）：不能视为求解完成"
    )))
}

/// 接受一个解的门槛：原问题相对违反量与目标相对差都不超过它。
///
/// 剪枝是**限制**：子问题即便可解，也可能已经不是原问题的最优解（甚至不满足原
/// 问题的约束）。所以每条返回路径都要用**原始 LP**复核一遍，过了才算求解完成。
const ACCEPT_VIOLATION: f64 = 1e-6;
const ACCEPT_OBJECTIVE_GAP: f64 = 1e-6;

/// 用**原始 LP**（未剪枝）评估一个解：返回 (最大相对约束违反量, 目标值)。
fn evaluate_solution(objective: &[(usize, f64)], rows: &[ParsedRow], values: &[f64]) -> (f64, f64) {
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
        // 按该行的**总量级**归一：取 max(|常数项|, Σ|系数×取值|)。
        // - 不用「行内最大项」：项数多时和会比最大项大一个量级，会把正常残差放大；
        // - 不加 1.0 下限：量级很小的行会被放大成虚假的「大违反」。
        let magnitude: f64 = row
            .terms
            .iter()
            .map(|(index, coeff)| (coeff * values[*index]).abs())
            .sum();
        let row_scale = magnitude.max(row.rhs.abs());
        let denominator = if row_scale > 0.0 { row_scale } else { 1.0 };
        max_violation = max_violation.max(raw / denominator);
    }
    let objective_value: f64 = objective
        .iter()
        .map(|(index, coeff)| coeff * values[*index])
        .sum();
    (max_violation, objective_value)
}

/// 组装最终结果（各条返回路径共用）。
fn assemble(
    values: Vec<f64>,
    prim_scales: AIndexMap<Variable, f64>,
    dual_scales: Vec<f64>,
    global_scale: f64,
    report: SolveReport,
    objective: &[(usize, f64)],
    orig_vars: &[Variable],
) -> RuizSolution {
    let cost = objective
        .iter()
        .map(|(index, coeff)| coeff * values[*index])
        .sum();
    let values_map: AIndexMap<Variable, f64> = orig_vars
        .iter()
        .zip(values.iter())
        .map(|(var, value)| (*var, *value))
        .collect();
    RuizSolution {
        values: values_map,
        prim_scales,
        dual_scales,
        cost,
        global_scale,
        report,
    }
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
