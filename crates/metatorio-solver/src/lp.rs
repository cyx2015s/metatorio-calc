//! 连续 LP 的应用适配：项目级后端选择与直接错误透出。
//!
//! 这里不再做求解后的原问题复核、证书拒绝或回退重试。所选后端返回什么
//! 状态就向调用方暴露什么状态：不可行、无界、数值失败、超时都由后端原样
//! 报告。需要更强保证的调用方应显式选用带证书的后端，而不是在适配层再判一次。
#[cfg(feature = "rust-lp")]
use std::time::Instant;

use crate::concept::AIndexMap;
use crate::lp_model::{
    Constraint, Expression, ProblemVariables, ResolutionError, Variable, VariableDefinition,
};
use crate::lp_solver::LpSolver;

/// 一次求解的记录（随结果返回给上层）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SolveReport {
    /// 参与求解的变量数。
    pub variables_before: usize,
    /// 解里非零的变量数（顶点解下就是支撑集大小）。
    pub variables_after: usize,
    /// 本次实际选用的后端。
    pub solver: LpSolver,
    /// 以下字段由仍带证书的 Rust based 后端填写；其他后端保持默认值。
    pub exact_primal_verified: bool,
    pub exact_optimality_verified: bool,
    pub certified_relative_gap: Option<f64>,
    pub relative_primal_verified: bool,
    pub maximum_relative_balance_error: Option<f64>,
    pub balance_tolerance: Option<f64>,
    pub relative_duality_gap: Option<f64>,
    pub optimization: Option<OptimizationReport>,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OptimizationTermination {
    Optimal,
    TimeLimit,
    #[default]
    SearchStopped,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PivotCostChange {
    pub pivot: usize,
    pub cost_before: f64,
    pub cost_after: f64,
    pub decrease: f64,
}

/// Evidence for the model searched, before any planner subset is written back.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OptimizationReport {
    pub optimality_proven: bool,
    pub termination_reason: OptimizationTermination,
    pub elapsed_seconds: f64,
    pub time_limit_seconds: f64,
    pub recent_pivot_cost_changes: Vec<PivotCostChange>,
    #[serde(default)]
    pub floating_search_pivots: Option<usize>,
    #[serde(default)]
    pub optimization_pivots: Option<usize>,
    #[serde(default)]
    pub improving_pivots: Option<usize>,
    #[serde(default)]
    pub optimization_elapsed_seconds: Option<f64>,
    #[serde(default)]
    pub first_feasible_seconds: Option<f64>,
}

/// 求解结果（原问题空间）。
pub struct LpSolution {
    values: AIndexMap<Variable, f64>,
    /// 上层按约束下标读取还原比例；两个后端都已还原原问题空间，恒为 1.0。
    pub dual_scales: Vec<f64>,
    pub cost: f64,
    /// 与 dual_scales 同理，恒为 1.0（保留给上层的取值还原路径）。
    pub global_scale: f64,
    /// 本次求解的记录（见 SolveReport）。
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
pub(crate) struct ParsedRow {
    pub(crate) is_equality: bool,
    pub(crate) rhs: f64,
    /// (变量下标, 系数)
    pub(crate) terms: Vec<(usize, f64)>,
}

#[cfg(any(feature = "clarabel", feature = "highs", feature = "microlp"))]
mod good_lp_backend {
    use super::*;
    #[cfg(feature = "clarabel")]
    use good_lp::solvers::clarabel::clarabel;
    #[cfg(feature = "highs")]
    use good_lp::solvers::highs::highs;
    #[cfg(feature = "microlp")]
    use good_lp::solvers::microlp::microlp;
    use good_lp::{Solution, Solver, SolverModel};

    /// 由解析后的 LP 构建出的 good_lp 问题。
    pub(super) struct BuiltProblem {
        pub(super) variables: good_lp::ProblemVariables,
        pub(super) vars_in_order: Vec<good_lp::Variable>,
        pub(super) objective: good_lp::Expression,
        pub(super) constraints: Vec<good_lp::Constraint>,
        /// 出现「不含任何变量、又不可能满足」的约束（例如目标物品根本不在任何流里，
        /// 目标约束退化成 0 == 1）→ 原问题不可行，不必交给后端。
        pub(super) trivially_infeasible: bool,
    }

    pub(super) fn build_problem(
        defs: &[VariableDefinition],
        objective: &[(usize, f64)],
        rows: &[ParsedRow],
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
                if (row.is_equality && row.rhs.abs() > 1e-12) || (!row.is_equality && row.rhs < 0.0)
                {
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
            vars_in_order,
            variables,
            objective: objective_expr,
            constraints,
            trivially_infeasible,
        }
    }

    pub(super) fn solve_with<S>(
        defs: &[VariableDefinition],
        objective: &[(usize, f64)],
        rows: &[ParsedRow],
        solver: S,
    ) -> Result<Vec<f64>, ResolutionError>
    where
        S: Solver,
        S::Model: SolverModel<Error = good_lp::ResolutionError>,
    {
        let full = build_problem(defs, objective, rows);
        if full.trivially_infeasible {
            return Err(ResolutionError::Infeasible);
        }
        let model = full.variables.minimise(full.objective).using(solver);
        let solution = model
            .with_all(full.constraints)
            .solve()
            .map_err(map_error)?;
        Ok(full
            .vars_in_order
            .iter()
            .map(|var| solution.value(*var))
            .collect())
    }

    fn map_error(error: good_lp::ResolutionError) -> ResolutionError {
        match error {
            good_lp::ResolutionError::Infeasible => ResolutionError::Infeasible,
            good_lp::ResolutionError::Unbounded => ResolutionError::Unbounded,
            good_lp::ResolutionError::Other(message) => ResolutionError::Other(message),
            good_lp::ResolutionError::Str(message) => ResolutionError::Str(message),
        }
    }

    pub(super) fn solve(
        solver: LpSolver,
        defs: &[VariableDefinition],
        objective: &[(usize, f64)],
        rows: &[ParsedRow],
    ) -> Result<Vec<f64>, ResolutionError> {
        match solver {
            LpSolver::Clarabel => {
                #[cfg(feature = "clarabel")]
                return solve_with(defs, objective, rows, clarabel);
                #[cfg(not(feature = "clarabel"))]
                return Err(ResolutionError::Str(solver.unavailable_reason()));
            }
            LpSolver::Highs => {
                #[cfg(feature = "highs")]
                return solve_with(defs, objective, rows, highs);
                #[cfg(not(feature = "highs"))]
                return Err(ResolutionError::Str(solver.unavailable_reason()));
            }
            LpSolver::Microlp => {
                #[cfg(feature = "microlp")]
                return solve_with(defs, objective, rows, microlp);
                #[cfg(not(feature = "microlp"))]
                return Err(ResolutionError::Str(solver.unavailable_reason()));
            }
            LpSolver::Soplex | LpSolver::RustBased => {
                Err(ResolutionError::Str(solver.unavailable_reason()))
            }
        }
    }
}

/// 把求解结果装入应用变量表。
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

/// 使用默认后端求解连续 LP；项目级调用走 solve_lp_named 并传入项目设置。
/// 默认值由编译进来的 feature 决定：优先 HiGHS，其次 Clarabel、microlp，
/// 最后才是 Rust based。
pub fn solve_lp(
    minimise: Expression,
    constraints: Vec<Constraint>,
    variables: ProblemVariables,
) -> Result<LpSolution, ResolutionError> {
    solve_lp_with_solver(minimise, constraints, variables, LpSolver::default())
}

/// 用显式后端求解，供测试、回放和诊断工具使用。
pub fn solve_lp_with_solver(
    minimise: Expression,
    constraints: Vec<Constraint>,
    variables: ProblemVariables,
    solver: LpSolver,
) -> Result<LpSolution, ResolutionError> {
    solve_lp_impl(minimise, constraints, variables, solver, Vec::new())
}

pub(crate) fn solve_lp_named(
    minimise: Expression,
    constraints: Vec<Constraint>,
    variables: ProblemVariables,
    solver: LpSolver,
    labels: Vec<String>,
) -> Result<LpSolution, ResolutionError> {
    solve_lp_impl(minimise, constraints, variables, solver, labels)
}

fn solve_lp_impl(
    minimise: Expression,
    constraints: Vec<Constraint>,
    variables: ProblemVariables,
    solver: LpSolver,
    labels: Vec<String>,
) -> Result<LpSolution, ResolutionError> {
    crate::control::check_cancelled().map_err(ResolutionError::Str)?;
    if !solver.is_available() {
        return Err(ResolutionError::Str(solver.unavailable_reason()));
    }
    #[cfg(not(feature = "rust-lp"))]
    let _ = labels;
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

    if defs.is_empty() {
        return Ok(assemble(
            Vec::new(),
            vec![1.0; rows.len()],
            minimise.constant(),
            SolveReport {
                solver,
                ..SolveReport::default()
            },
            &orig_vars,
        ));
    }

    #[cfg(feature = "rust-lp")]
    if solver == LpSolver::RustBased {
        return solve_rust_backend(minimise, defs, orig_vars, objective, rows, labels, solver);
    }

    #[cfg(feature = "soplex")]
    if solver == LpSolver::Soplex {
        let values = crate::soplex_backend::solve(&defs, &objective, &rows)?;
        crate::control::check_cancelled().map_err(ResolutionError::Str)?;
        let cost = objective
            .iter()
            .map(|(index, coeff)| coeff * values[*index])
            .sum::<f64>()
            + minimise.constant();
        let variables_used = values.iter().filter(|value| **value != 0.0).count();
        return Ok(assemble(
            values,
            vec![1.0; rows.len()],
            cost,
            SolveReport {
                variables_before: defs.len(),
                variables_after: variables_used,
                solver,
                ..SolveReport::default()
            },
            &orig_vars,
        ));
    }

    #[cfg(any(feature = "clarabel", feature = "highs", feature = "microlp"))]
    {
        let values = good_lp_backend::solve(solver, &defs, &objective, &rows)?;
        crate::control::check_cancelled().map_err(ResolutionError::Str)?;
        let cost = objective
            .iter()
            .map(|(index, coeff)| coeff * values[*index])
            .sum::<f64>()
            + minimise.constant();
        let variables_used = values.iter().filter(|value| **value != 0.0).count();
        Ok(assemble(
            values,
            vec![1.0; rows.len()],
            cost,
            SolveReport {
                variables_before: defs.len(),
                variables_after: variables_used,
                solver,
                ..SolveReport::default()
            },
            &orig_vars,
        ))
    }

    #[cfg(not(any(feature = "clarabel", feature = "highs", feature = "microlp")))]
    Err(ResolutionError::Str(solver.unavailable_reason()))
}

#[cfg(feature = "rust-lp")]
fn solve_rust_backend(
    minimise: Expression,
    defs: Vec<VariableDefinition>,
    orig_vars: Vec<Variable>,
    objective: Vec<(usize, f64)>,
    rows: Vec<ParsedRow>,
    labels: Vec<String>,
    solver: LpSolver,
) -> Result<LpSolution, ResolutionError> {
    use crate::lp_experiments::{Row, Snapshot, capture, column};

    let mut objective_terms: Vec<_> = objective.iter().map(|(i, a)| (*i, a.to_bits())).collect();
    objective_terms.sort_by_key(|(i, _)| *i);
    let snapshot = Snapshot {
        format_version: 1,
        columns: defs.iter().enumerate().map(|(i, d)| column(d, i)).collect(),
        rows: rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let mut terms: Vec<_> = row.terms.iter().map(|(i, a)| (*i, a.to_bits())).collect();
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
        production_rhs_scale_bits: 1.0f64.to_bits(),
    };
    let captured = capture(&snapshot);
    let start = Instant::now();
    let result = crate::lp_hybrid::solve_rust_certified(&snapshot, captured.as_deref())
        .map_err(crate::lp_hybrid::Failure::resolution_error)?;
    crate::control::check_cancelled().map_err(ResolutionError::Str)?;
    let variables_used = result.values.iter().filter(|x| **x != 0.0).count();
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
    let cost = objective
        .iter()
        .map(|(index, coeff)| coeff * result.values[*index])
        .sum::<f64>()
        + minimise.constant();
    eprintln!(
        "[LP-EXACT backend={solver}] columns={} support={} pivots={} full_primal={} full_optimal={} elapsed={:.2}s",
        defs.len(),
        variables_used,
        result.pivots,
        result.certificate.primal_feasible,
        result.certificate.optimal,
        start.elapsed().as_secs_f64()
    );
    Ok(assemble(
        result.values,
        vec![1.0; rows.len()],
        cost,
        SolveReport {
            variables_before: defs.len(),
            variables_after: variables_used,
            solver,
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
            optimization: result.optimization,
        },
        &orig_vars,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lp_model::*;

    #[test]
    fn backend_names_round_trip_and_availability_matches_features() {
        for solver in LpSolver::ALL {
            assert_eq!(solver.to_string(), solver.as_str());
            assert_eq!(
                solver.is_available(),
                match solver {
                    LpSolver::Clarabel => cfg!(feature = "clarabel"),
                    LpSolver::Highs => cfg!(feature = "highs"),
                    LpSolver::Microlp => cfg!(feature = "microlp"),
                    LpSolver::Soplex => cfg!(feature = "soplex"),
                    LpSolver::RustBased => cfg!(feature = "rust-lp"),
                }
            );
        }
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

    #[test]
    fn every_compiled_backend_solves_a_simple_model() {
        for solver in LpSolver::ALL {
            if !solver.is_available() {
                continue;
            }
            let mut vars = ProblemVariables::new();
            let x = vars.add(variable().min(0));
            let solution = solve_lp_with_solver(
                x.into_expression(),
                vec![x.into_expression().geq(1.0)],
                vars,
                solver,
            )
            .unwrap_or_else(|error| panic!("{solver}: {error}"));
            assert!((solution.value(x) - 1.0).abs() < 1e-7, "{solver}");
            assert_eq!(solution.report.solver, solver);
        }
    }

    #[cfg(feature = "highs")]
    #[test]
    fn infeasible_and_unbounded_are_not_rewritten_as_other_errors() {
        let mut vars = ProblemVariables::new();
        let x = vars.add(variable().min(0));
        let infeasible = solve_lp_with_solver(
            x.into_expression(),
            vec![x.into_expression().eq(1.0), x.into_expression().eq(2.0)],
            vars,
            LpSolver::Highs,
        );
        assert!(matches!(infeasible, Err(ResolutionError::Infeasible)));

        let mut vars = ProblemVariables::new();
        let x = vars.add(variable().min(0));
        let unbounded = solve_lp_with_solver(-1.0 * x, vec![], vars, LpSolver::Highs);
        assert!(matches!(unbounded, Err(ResolutionError::Unbounded)));
    }
}
