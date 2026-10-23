//! Small, opt-in regression probes for backend variable bounds.
//! Run with --features highs -- --ignored.
//! These probes exercise the adapter directly; there is no post-solve validation.

use metatorio_solver::{
    LpSolver,
    lp_model::{IntoAffineExpression, ProblemVariables, variable},
    solve_lp_with_solver,
};

#[test]
#[ignore = "audit control: opt in with an explicit LP backend"]
fn nonzero_rhs_with_homogeneous_bounds() {
    let mut vars = ProblemVariables::new();
    let x = vars.add(variable().min(0.0));
    let solution = solve_lp_with_solver(
        x.into_expression(),
        vec![x.into_expression().geq(1.0)],
        vars,
        LpSolver::Highs,
    )
    .unwrap();
    assert!((solution.value(x) - 1.0).abs() < 1e-10);
}

#[test]
#[ignore = "audit regression: opt in with an explicit LP backend"]
fn lower_bound_without_rhs() {
    let mut vars = ProblemVariables::new();
    let x = vars.add(variable().min(1.0));
    let solution =
        solve_lp_with_solver(x.into_expression(), vec![], vars, LpSolver::Highs).unwrap();
    assert!((solution.value(x) - 1.0).abs() < 1e-10);
}

#[test]
#[ignore = "audit regression: exposes unscaled lower bound in HiGHS"]
fn lower_bound_with_nonzero_rhs() {
    let mut vars = ProblemVariables::new();
    let x = vars.add(variable().min(1.0));
    let y = vars.add(variable().min(0.0));
    let solution = solve_lp_with_solver(
        x.into_expression(),
        vec![y.into_expression().eq(1.0)],
        vars,
        LpSolver::Highs,
    )
    .unwrap();
    assert!((solution.value(x) - 1.0).abs() < 1e-10);
}

#[test]
#[ignore = "audit regression: exposes unscaled upper bound in HiGHS"]
fn upper_bound_with_nonzero_rhs() {
    let mut vars = ProblemVariables::new();
    let x = vars.add(variable().min(0.0).max(2.0));
    let solution = solve_lp_with_solver(
        x.into_expression(),
        vec![x.into_expression().eq(1.0)],
        vars,
        LpSolver::Highs,
    )
    .unwrap_or_else(|error| panic!("feasible x=1 was rejected: {error}"));
    assert!((solution.value(x) - 1.0).abs() < 1e-10);
}
