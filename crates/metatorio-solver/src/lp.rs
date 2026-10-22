//! 连续 LP 的应用适配：可选 HiGHS 与 Rust 浮点搜索/精确认证后端。
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
use crate::lp_model::{
    Constraint, Expression, ProblemVariables, ResolutionError, Variable, VariableDefinition,
};
#[cfg(feature = "highs")]
use good_lp::{Solution, SolverModel, solvers::highs::highs};

/// 一次求解的记录（随结果返回给上层）。
///
/// 应用没有安装 logger（`log` 宏是空操作），所以关键决策要随结果留痕。
/// 保留原问题校验、支撑集大小，以及 Rust 后端的精确可行性和目标差距证据。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SolveReport {
    /// 参与求解的变量数。
    pub variables_before: usize,
    /// 解里非零的变量数（顶点解下就是支撑集大小）。
    pub variables_after: usize,
    /// 用**原始 LP**评估这个解的最大相对约束违反量。
    pub primal_violation: f64,
    /// Full original LP verified with exact binary-rational coefficients.
    pub exact_primal_verified: bool,
    /// Full original LP has an exact primal/dual optimality certificate.
    pub exact_optimality_verified: bool,
    /// Upward-rounded full-model exact gap / max(1, abs(primal objective)).
    pub certified_relative_gap: Option<f64>,
    /// Full-model relative row balance, including the actual rounded output.
    pub relative_primal_verified: bool,
    pub maximum_relative_balance_error: Option<f64>,
    pub balance_tolerance: Option<f64>,
    /// Stopping gap for a toleranced primal point; not an exact optimum bound.
    pub relative_duality_gap: Option<f64>,
}

/// 求解结果（原问题空间）。
pub struct LpSolution {
    values: AIndexMap<Variable, f64>,
    /// 上层按约束下标读取还原比例；两个后端都已还原原问题空间，恒为 1.0。
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

    /// 后端已经还原原问题空间的值，恒为 1.0。
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
#[cfg(feature = "highs")]
struct BuiltProblem {
    variables: good_lp::ProblemVariables,
    vars_in_order: Vec<good_lp::Variable>,
    objective: good_lp::Expression,
    constraints: Vec<good_lp::Constraint>,
    /// 出现「不含任何变量、又不可能满足」的约束（例如目标物品根本不在任何流里，
    /// 目标约束退化成 0 == 1）→ 原问题不可行，不必交给后端。
    trivially_infeasible: bool,
}

/// 把解析后的 `objective/rows` 重建成一个 good_lp 问题。
///
/// `rhs_scale`：所有非零右端统一乘的系数（见 [`TARGET_CANONICAL_SCALE`]）。
/// 只作用在约束右端；`trivially_infeasible` 的判定仍用原始 `row.rhs`。
#[cfg(feature = "highs")]
fn build_problem(
    defs: &[VariableDefinition],
    objective: &[(usize, f64)],
    rows: &[ParsedRow],
    rhs_scale: f64,
) -> BuiltProblem {
    let mut variables = good_lp::ProblemVariables::new();
    let vars_in_order: Vec<_> = defs
        .iter()
        .map(|d| {
            variables.add(
                good_lp::variable()
                    .min(d.get_min())
                    .max(d.get_max())
                    .name(d.get_name()),
            )
        })
        .collect();
    let fold = |terms: &[(usize, f64)]| -> good_lp::Expression {
        terms
            .iter()
            .map(|(index, coeff)| vars_in_order[*index] * *coeff)
            .fold(good_lp::Expression::from(0.0), |acc, term| acc + term)
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
        let rhs = row.rhs * rhs_scale;
        let constraint = if row.is_equality {
            expr.eq(rhs)
        } else {
            expr.leq(rhs)
        };
        constraints.push(constraint);
    }
    BuiltProblem {
        vars_in_order,
        variables,
        objective: objective_expr,
        constraints,
        trivially_infeasible,
    }
}

/// 求解失败时换策略重试的阶梯（第一项 = 默认选项）。
///
/// # 为什么需要
///
/// HiGHS 的单纯形在退化模型上会偶发**循环**（cycling），此时它会主动放弃求解并
/// 返回 kUnknown / NoSolutionFound 这类**非终止**状态——它**不代表无可行解**。
/// （HiGHS 作者在 ERGO-Code/HiGHS#1528 的结论就是「换个策略重跑」。）
///
/// 真实案例：带插件塔的两个科研瓶目标，默认选项稳定报「内部错误」；关掉均衡缩放
/// （simplex_scale_strategy=0）后两个都能解出；再不行就同时关 presolve。
/// 这些选项只影响**数值路径**，不改变模型，解仍要过原问题校验才会被采用。
#[derive(Clone, Copy, Debug)]
#[cfg(feature = "highs")]
enum SolverAttempt {
    /// 默认策略（presolve 与均衡缩放都由 HiGHS 自己选）。
    Default,
    /// 关掉均衡缩放：换一条枢轴路径，绕开循环。
    NoScaling,
    /// 同时关掉 presolve 与缩放：前两组都失败时的兜底。
    NoScalingNoPresolve,
}

#[cfg(feature = "highs")]
const SOLVER_ATTEMPTS: &[SolverAttempt] = &[
    SolverAttempt::Default,
    SolverAttempt::NoScaling,
    SolverAttempt::NoScalingNoPresolve,
];

/// 解 LP；遇到非终止失败按 SOLVER_ATTEMPTS 换策略重试。
///
/// 每次重试都重建模型：good_lp 的 variables/constraints 是按值消耗的，重建比克隆
/// 直白，代价也远小于一次求解。Infeasible / Unbounded 是**定论**，不重试。
#[cfg(feature = "highs")]
fn solve_with_retry(
    defs: &[VariableDefinition],
    objective: &[(usize, f64)],
    rows: &[ParsedRow],
    rhs_scale: f64,
) -> Result<Vec<f64>, ResolutionError> {
    let mut last: Option<ResolutionError> = None;
    for (index, attempt) in SOLVER_ATTEMPTS.iter().enumerate() {
        crate::control::check_cancelled().map_err(ResolutionError::Str)?;
        let full = build_problem(defs, objective, rows, rhs_scale);
        let mut model = full
            .variables
            .minimise(full.objective)
            .using(highs)
            // 默认 output_flag = true 会把 HiGHS 日志打到控制台，关掉。
            .set_option("output_flag", false);
        match attempt {
            SolverAttempt::Default => {}
            SolverAttempt::NoScaling => {
                model = model.set_option("simplex_scale_strategy", 0);
            }
            SolverAttempt::NoScalingNoPresolve => {
                model = model.set_option("presolve", "off");
                model = model.set_option("simplex_scale_strategy", 0);
            }
        }
        match model.with_all(full.constraints).solve() {
            Ok(solution) => {
                if index > 0 {
                    log::warn!("求解在默认选项下失败，改用 {attempt:?} 后成功");
                }
                return Ok(full
                    .vars_in_order
                    .iter()
                    .map(|v| solution.value(*v))
                    .collect());
            }
            Err(error) => {
                let error = match error {
                    good_lp::ResolutionError::Infeasible => ResolutionError::Infeasible,
                    good_lp::ResolutionError::Unbounded => ResolutionError::Unbounded,
                    good_lp::ResolutionError::Other(s) => ResolutionError::Other(s),
                    good_lp::ResolutionError::Str(s) => ResolutionError::Str(s),
                };
                // 无可行解 / 无界是定论：换策略没有意义。
                let retryable =
                    matches!(error, ResolutionError::Other(_) | ResolutionError::Str(_));
                if retryable && index + 1 < SOLVER_ATTEMPTS.len() {
                    log::warn!(
                        "求解失败（第 {} 次尝试，{attempt:?}）：{error:?}；换策略重试",
                        index + 1
                    );
                }
                last = Some(error);
                if !retryable {
                    break;
                }
            }
        }
    }
    Err(last.unwrap_or(ResolutionError::Other("求解未执行")))
}

/// 目标行归一化：把 LP 里最大的非零右端放大到至少这个量级再交给 HiGHS。
///
/// # 为什么需要
///
/// HiGHS 的可行性容差是**绝对**的（`primal_feasibility_tolerance`，默认 1e-7，
/// 且作用在均衡缩放后的空间）。一条相对目标只有 ~1e-11 的**必需**小链，在目标
/// 量级很小时会被求解器合法地压成精确 0（残差落在容差内），于是顶点解的支撑集
/// 里有消费者、没有生产者，回写出去的文档就不可行。
///
/// 本 LP 除目标行外右端全为 0，所以把非零右端整体乘 `rhs_scale` 等价于把**整个
/// 解**乘 `rhs_scale`（正齐次）；解完再除回来即可，是精确变换。只放大、不缩小：
/// 目标本来很大时保持原样（缩小反而会把别的流量压到容差之下）。
const TARGET_CANONICAL_SCALE: f64 = 1e4;
/// 放大倍数的上限，避免目标小到离谱时溢出。
const MAX_RHS_SCALE: f64 = 1e12;

/// 接受一个解的门槛：原问题最大相对约束违反量不超过它。
const ACCEPT_VIOLATION: f64 = 1e-6;
/// 绝对可行性余量：相对违反量之外再给一个绝对下限（见 [`evaluate_solution`]）。
const ABSOLUTE_VIOLATION: f64 = 1e-8;

fn select_backend(requested: Option<&str>) -> Result<&str, ResolutionError> {
    let name = match requested {
        Some(name) => name,
        None if cfg!(feature = "rust-lp") => "rust-certified",
        None if cfg!(feature = "highs") => "highs",
        None => return Err(ResolutionError::Other("no LP backend enabled")),
    };
    match name {
        "highs" if cfg!(feature = "highs") => Ok(name),
        "rust-certified" | "rust-verified" | "rust-exact" if cfg!(feature = "rust-lp") => Ok(name),
        "hybrid-exact" if cfg!(feature = "rust-lp") && cfg!(feature = "highs") => Ok(name),
        "highs" | "hybrid-exact" | "rust-certified" | "rust-verified" | "rust-exact" => Err(
            ResolutionError::Str(format!("LP backend {name} is unavailable in this build")),
        ),
        _ => Err(ResolutionError::Str(format!("unknown LP backend: {name}"))),
    }
}

/// 用**原始 LP**评估一个解：返回 (最大相对约束违反量, 目标值)。
///
/// 相对量按该行的总量级归一：`max(|rhs|, Σ|系数×取值|)`。再叠加一个绝对下限：
/// 没有它，`0 = 0` 这种**近乎空行**（rhs = 0、取值也只有 ~1e-12）会把残差按
/// 自身量级归一成「相对违反 1.0」，把本来正确的解误杀。
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

/// 使用所选后端求解连续 LP。启用 rust-lp 的构建默认选择 Rust 认证路径；
/// 仅启用 highs 时默认选择 HiGHS。计算未完成与已经证明的不可行分别返回错误，
/// 所有返回值都须满足应用对原问题空间的校验。
pub fn solve_lp(
    minimise: Expression,
    constraints: Vec<Constraint>,
    variables: ProblemVariables,
) -> Result<LpSolution, ResolutionError> {
    solve_lp_impl(
        minimise,
        constraints,
        variables,
        #[cfg(feature = "rust-lp")]
        Vec::new(),
    )
}

#[cfg(feature = "rust-lp")]
pub(crate) fn solve_lp_named(
    minimise: Expression,
    constraints: Vec<Constraint>,
    variables: ProblemVariables,
    labels: Vec<String>,
) -> Result<LpSolution, ResolutionError> {
    solve_lp_impl(minimise, constraints, variables, labels)
}

fn solve_lp_impl(
    minimise: Expression,
    constraints: Vec<Constraint>,
    variables: ProblemVariables,
    #[cfg(feature = "rust-lp")] labels: Vec<String>,
) -> Result<LpSolution, ResolutionError> {
    crate::control::check_cancelled().map_err(ResolutionError::Str)?;
    let defs: Vec<VariableDefinition> = variables
        .iter_variables_with_def()
        .map(|(_, def)| def.clone())
        .collect();
    let orig_vars: Vec<Variable> = variables
        .iter_variables_with_def()
        .map(|(var, _)| var)
        .collect();
    for expression in std::iter::once(&minimise).chain(constraints.iter().map(|c| c.expression())) {
        if !expression.constant().is_finite()
            || expression
                .linear_coefficients()
                .any(|(v, a)| v.0 >= defs.len() || !a.is_finite())
        {
            return Err(ResolutionError::Other(
                "invalid LP expression: nonfinite value or undeclared variable",
            ));
        }
    }
    // Backend-independent variable numbering follows declaration order.
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

    // 目标行归一化（见 TARGET_CANONICAL_SCALE）：只放大、不缩小。
    let max_rhs = rows.iter().fold(0.0f64, |acc, row| acc.max(row.rhs.abs()));
    let rhs_scale = if max_rhs > 0.0 {
        (TARGET_CANONICAL_SCALE / max_rhs).clamp(1.0, MAX_RHS_SCALE)
    } else {
        1.0
    };
    let requested = match std::env::var("METATORIO_LP_BACKEND") {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(ResolutionError::Other(
                "invalid LP backend environment value",
            ));
        }
    };
    let backend = select_backend(requested.as_deref())?;
    #[cfg(feature = "rust-lp")]
    if std::env::var_os("METATORIO_LP_CAPTURE_DIR").is_some()
        || matches!(
            backend,
            "hybrid-exact" | "rust-exact" | "rust-verified" | "rust-certified"
        )
    {
        use crate::lp_experiments::{Row, Snapshot, capture, column};
        let mut objective_terms: Vec<_> =
            objective.iter().map(|(i, a)| (*i, a.to_bits())).collect();
        objective_terms.sort_by_key(|(i, _)| *i);
        let snapshot = Snapshot {
            format_version: 1,
            columns: defs.iter().enumerate().map(|(i, d)| column(d, i)).collect(),
            rows: rows
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    let mut terms: Vec<_> =
                        row.terms.iter().map(|(i, a)| (*i, a.to_bits())).collect();
                    terms.sort_by_key(|(i, _)| *i);
                    Row {
                        label: labels.get(i).cloned().unwrap_or_else(|| format!("r{i}")),
                        equality: row.is_equality,
                        rhs_bits: row.rhs.to_bits(),
                        terms,
                    }
                })
                .collect(),
            objective: objective_terms,
            objective_offset_bits: minimise.constant().to_bits(),
            production_rhs_scale_bits: rhs_scale.to_bits(),
        };
        let captured = capture(&snapshot);
        if matches!(
            backend,
            "hybrid-exact" | "rust-exact" | "rust-verified" | "rust-certified"
        ) {
            let start = Instant::now();
            let result = if backend == "rust-exact" {
                crate::lp_hybrid::solve_rust(&snapshot)
                    .map_err(crate::lp_hybrid::Failure::resolution_error)
            } else if backend == "rust-verified" {
                crate::lp_hybrid::solve_rust_verified(&snapshot, captured.as_deref())
                    .map_err(crate::lp_hybrid::Failure::resolution_error)
            } else if backend == "rust-certified" {
                crate::lp_hybrid::solve_rust_certified(&snapshot, captured.as_deref())
                    .map_err(crate::lp_hybrid::Failure::resolution_error)
            } else {
                #[cfg(feature = "highs")]
                {
                    crate::lp_hybrid::solve(&snapshot, captured.as_deref())
                        .map_err(ResolutionError::Str)
                }
                #[cfg(not(feature = "highs"))]
                {
                    Err(ResolutionError::Other(
                        "hybrid-exact requires the highs feature",
                    ))
                }
            }?;
            crate::control::check_cancelled().map_err(ResolutionError::Str)?;
            let used = result.values.iter().filter(|x| **x != 0.0).count();
            let balance_error = result
                .relative
                .as_ref()
                .map(|(a, b)| {
                    Ok::<_, String>(
                        a.balance_error_upper_bound()?
                            .max(b.balance_error_upper_bound()?),
                    )
                })
                .transpose()
                .map_err(ResolutionError::Str)?;
            let relative_gap = result
                .relative
                .as_ref()
                .map(|(a, b)| {
                    Ok::<_, String>(
                        a.duality_gap_upper_bound()?
                            .ok_or("missing relative duality gap")?
                            .max(
                                b.duality_gap_upper_bound()?
                                    .ok_or("missing rounded relative duality gap")?,
                            ),
                    )
                })
                .transpose()
                .map_err(ResolutionError::Str)?;
            let (violation, cost) = evaluate_solution(&objective, &rows, &result.values);
            let cost = cost + minimise.constant();
            if violation > ACCEPT_VIOLATION {
                return Err(ResolutionError::Str(format!(
                    "Rust 精确解转回浮点后未通过应用校验：{violation:.3e}"
                )));
            }
            eprintln!(
                "[LP-EXACT backend={backend}] columns={} support={} pivots={} full_primal={} full_optimal={} elapsed={:.2}s",
                defs.len(),
                used,
                result.pivots,
                result.certificate.primal_feasible,
                result.certificate.optimal,
                start.elapsed().as_secs_f64()
            );
            return Ok(assemble(
                result.values,
                vec![1.0; rows.len()],
                cost,
                SolveReport {
                    variables_before: defs.len(),
                    variables_after: used,
                    primal_violation: violation,
                    exact_primal_verified: result.certificate.primal_feasible,
                    exact_optimality_verified: result.certificate.optimal,
                    certified_relative_gap: result
                        .certificate
                        .relative_gap_upper_bound()
                        .map_err(ResolutionError::Str)?,
                    relative_primal_verified: result
                        .relative
                        .as_ref()
                        .is_some_and(|(a, b)| a.accepted && b.accepted),
                    maximum_relative_balance_error: balance_error,
                    balance_tolerance: balance_error.map(|_| 1e-12),
                    relative_duality_gap: relative_gap,
                },
                &orig_vars,
            ));
        }
    }
    #[cfg(not(feature = "highs"))]
    {
        Err(ResolutionError::Str(format!(
            "LP backend {backend} is unavailable in this build"
        )))
    }
    #[cfg(feature = "highs")]
    {
        debug_assert_eq!(backend, "highs");
        let full = build_problem(&defs, &objective, &rows, rhs_scale);
        if full.trivially_infeasible {
            return Err(ResolutionError::Infeasible);
        }
        if defs.is_empty() {
            return Ok(assemble(
                Vec::new(),
                vec![1.0; rows.len()],
                minimise.constant(),
                SolveReport::default(),
                &orig_vars,
            ));
        }

        let instant = Instant::now();
        let solution = solve_with_retry(&defs, &objective, &rows, rhs_scale)?;
        crate::control::check_cancelled().map_err(ResolutionError::Str)?;
        // 求解在放大后的空间进行，这里除回原问题量级（rhs_scale = 1.0 时是无操作）。
        let values: Vec<f64> = solution.iter().map(|value| value / rhs_scale).collect();
        if values.iter().any(|value| !value.is_finite()) {
            return Err(ResolutionError::Other("HiGHS 返回非有限解"));
        }
        // **原问题校验**：HiGHS 有 presolve + postsolve，解未必在原空间完全满足约束。
        let (violation, cost) = evaluate_solution(&objective, &rows, &values);
        let cost = cost + minimise.constant();
        if violation > ACCEPT_VIOLATION {
            return Err(ResolutionError::Str(format!(
                "HiGHS 解未通过原问题校验（最大相对违反 {violation:.3e}）：不能视为求解完成"
            )));
        }
        let variables_used = values.iter().filter(|value| value.abs() > 0.0).count();
        log::info!(
            "HiGHS 求解完成：{} 个变量，支撑集 {} 个，目标右端放大 {:.3e}×，耗时 {:.2?}",
            defs.len(),
            variables_used,
            rhs_scale,
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
                ..SolveReport::default()
            },
            &orig_vars,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lp_model::*;

    #[test]
    fn backend_selection_prefers_certified_rust_and_rejects_unavailable_choices() {
        if cfg!(feature = "rust-lp") {
            assert_eq!(select_backend(None).unwrap(), "rust-certified");
        } else if cfg!(feature = "highs") {
            assert_eq!(select_backend(None).unwrap(), "highs");
        }
        for (name, available) in [
            ("highs", cfg!(feature = "highs")),
            ("rust-certified", cfg!(feature = "rust-lp")),
            ("rust-verified", cfg!(feature = "rust-lp")),
            ("rust-exact", cfg!(feature = "rust-lp")),
            (
                "hybrid-exact",
                cfg!(feature = "rust-lp") && cfg!(feature = "highs"),
            ),
        ] {
            assert_eq!(select_backend(Some(name)).is_ok(), available, "{name}");
        }
        assert!(select_backend(Some("ruts-certified")).is_err());
    }

    #[test]
    fn objective_constant_and_invalid_expression_are_preserved() {
        let mut vars = ProblemVariables::new();
        let x = vars.add(variable().min(0));
        let solution = solve_lp(2.0 * x + 7.0, vec![x.into_expression().eq(3.0)], vars).unwrap();
        assert_eq!(solution.cost, 13.0);
        assert!(matches!(
            solve_lp(Expression::from(f64::NAN), vec![], ProblemVariables::new()),
            Err(ResolutionError::Other(_))
        ));
        assert!(matches!(
            solve_lp(x.into_expression(), vec![], ProblemVariables::new()),
            Err(ResolutionError::Other(_))
        ));
    }
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
        assert!(
            (support[0] - 1.0).abs() < 1e-6,
            "该列取值应为 1：{support:?}"
        );
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

    /// 目标量级远小于 HiGHS 绝对可行容差（1e-7）时，必须在放大后的空间求解、
    /// 再除回原尺度。没有这层归一化，1e-9 的等式会被当成"在容差内"而解成 0：
    /// 这正是「目标 1/s 的必需小链被压成 0、回写后不可行」的机制。
    #[test]
    fn tiny_target_is_solved_in_scaled_space() {
        let mut vars = ProblemVariables::new();
        let x = vars.add(variable().min(0.0));
        let y = vars.add(variable().min(0.0));
        let constraints = vec![x.into_expression().eq(1e-9), (y - x).eq(0.0)];
        let solution = solve_lp(x + y, constraints, vars).expect("应可解");
        let xv = solution.value(x);
        let yv = solution.value(y);
        assert!(xv > 0.0, "x 被解成 0（未放大目标右端）：{xv}");
        assert!((xv / 1e-9 - 1.0).abs() < 1e-6, "x 应解回 1e-9：{xv}");
        assert!((yv / 1e-9 - 1.0).abs() < 1e-6, "y 应解回 1e-9：{yv}");
    }
}
