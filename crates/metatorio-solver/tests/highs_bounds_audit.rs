//! Small, opt-in regression probes for the HiGHS RHS scaling adapter.
//! Run with METATORIO_LP_BACKEND=highs and --features highs -- --ignored.
//! The same probes can be run with METATORIO_LP_BACKEND=rust-certified
//! using the default features (no native backend build required).

use metatorio_solver::lp::solve_lp;
use metatorio_solver::lp_model::{IntoAffineExpression, ProblemVariables, variable};

#[test]
#[ignore = "audit control: opt in with an explicit LP backend"]
fn nonzero_rhs_with_homogeneous_bounds() {
    let mut vars = ProblemVariables::new();
    let x = vars.add(variable().min(0.0));
    let solution = solve_lp(
        x.into_expression(),
        vec![x.into_expression().geq(1.0)],
        vars,
    )
    .unwrap();
    assert!((solution.value(x) - 1.0).abs() < 1e-10);
}

#[test]
#[ignore = "audit regression: opt in with an explicit LP backend"]
fn lower_bound_without_rhs() {
    let mut vars = ProblemVariables::new();
    let x = vars.add(variable().min(1.0));
    let solution = solve_lp(x.into_expression(), vec![], vars).unwrap();
    assert!((solution.value(x) - 1.0).abs() < 1e-10);
}

#[test]
#[ignore = "audit regression: exposes unscaled lower bound in HiGHS"]
fn lower_bound_with_nonzero_rhs() {
    let mut vars = ProblemVariables::new();
    let x = vars.add(variable().min(1.0));
    let y = vars.add(variable().min(0.0));
    let solution = solve_lp(x.into_expression(), vec![y.into_expression().eq(1.0)], vars).unwrap();
    eprintln!(
        "x={}, y={}, report_violation={}",
        solution.value(x),
        solution.value(y),
        solution.report.primal_violation
    );
    assert!((solution.value(x) - 1.0).abs() < 1e-10);
}

#[test]
#[ignore = "audit regression: exposes unscaled upper bound in HiGHS"]
fn upper_bound_with_nonzero_rhs() {
    let mut vars = ProblemVariables::new();
    let x = vars.add(variable().min(0.0).max(2.0));
    let result = solve_lp(x.into_expression(), vec![x.into_expression().eq(1.0)], vars);
    let solution = result.unwrap_or_else(|error| panic!("feasible x=1 was rejected: {error}"));
    assert!((solution.value(x) - 1.0).abs() < 1e-10);
}
