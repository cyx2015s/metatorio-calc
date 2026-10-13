//! Experimental application bridge: HiGHS proposes support/basis, Rust verifies
//! and repairs. A feasible result does not imply full-model optimality.
use crate::{
    lp_experiments::{ReplayOptions, Snapshot, replay},
    lp_rational as exact,
};
use num_traits::Zero;
use std::time::Instant;

pub(crate) struct Result {
    pub values: Vec<f64>,
    pub certificate: exact::Certificate,
    pub pivots: usize,
}

/// Pure Rust cold start; no native candidate search or fallback.
pub(crate) fn solve_rust(snapshot: &Snapshot) -> std::result::Result<Result, String> {
    snapshot.check()?;
    let model = snapshot.exact_model();
    let start = Instant::now();
    let result =
        model.solve_dual_with_observer(10000, |_| start.elapsed().as_secs_f64() < 120.0)?;
    let certificate = model.certify(&result.solution)?;
    if !certificate.optimal {
        return Err(format!(
            "Rust cold start incomplete: {:?}; exact infeasibility={}",
            result.stop_reason,
            result.infeasibility.is_some()
        ));
    }
    validated_result(snapshot, result.solution, certificate, result.pivots)
}

/// Native-free floating search, followed by exact full-model correction.
pub(crate) fn solve_rust_verified(
    snapshot: &Snapshot,
    captured: Option<&std::path::Path>,
) -> std::result::Result<Result, String> {
    snapshot.check()?;
    let model = snapshot.exact_model();
    let start = Instant::now();
    let proposal =
        model.float_candidate_with_observer(20000, |_| start.elapsed().as_secs_f64() < 120.0)?;
    if let Some(source) = captured {
        let save = || -> std::result::Result<(), Box<dyn std::error::Error>> {
            let directory = source
                .parent()
                .ok_or("capture has no parent")?
                .join("rust-candidates");
            std::fs::create_dir_all(&directory)?;
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(directory.join(source.file_name().ok_or("capture has no filename")?))?;
            serde_json::to_writer(
                std::io::BufWriter::new(file),
                &serde_json::json!({
                    "original_snapshot":source,"model_scope":"full","basis":proposal.basis,
                    "pivots":proposal.pivots,"stop_reason":proposal.stop_reason,
                    "diagnostic":proposal.diagnostic,"proposal_seconds":start.elapsed().as_secs_f64(),
                }),
            )?;
            Ok(())
        };
        if let Err(error) = save() {
            eprintln!("[LP-RUST-CANDIDATE-CAPTURE-ERROR] {error}");
        }
    }
    let result = model.refine_basis_with_observer(proposal.basis, 1000, |_, _| {
        start.elapsed().as_secs_f64() < 120.0
    })?;
    if !result.certificate.optimal {
        return Err(format!(
            "Rust verified solve incomplete: {:?}; exact infeasibility={}",
            result.stop_reason,
            result.infeasibility.is_some()
        ));
    }
    validated_result(
        snapshot,
        result.solution,
        result.certificate,
        proposal.pivots + result.repair_pivots + result.optimization_pivots,
    )
}

fn validated_result(
    snapshot: &Snapshot,
    solution: exact::ExactSolution,
    certificate: exact::Certificate,
    pivots: usize,
) -> std::result::Result<Result, String> {
    if !certificate.primal_feasible {
        return Err("exact full-model verification failed".into());
    }
    let values = solution.approximate()?;
    let rounded = snapshot.validate(&values);
    if !rounded.finite
        || rounded.maximum_relative_violation != 0.0
        || rounded.maximum_bound_violation != 0.0
    {
        return Err("rounded exact solution exceeds original model budgets".into());
    }
    Ok(Result {
        values,
        certificate,
        pivots,
    })
}

pub(crate) fn solve(
    snapshot: &Snapshot,
    captured: Option<&std::path::Path>,
) -> std::result::Result<Result, String> {
    snapshot.check()?;
    let options = ReplayOptions {
        name: "hybrid-seed".into(),
        rhs_scale: f64::from_bits(snapshot.production_rhs_scale_bits),
        solver: "simplex".into(),
        scaling: None,
        presolve: true,
        tolerance: None,
        small_matrix_value: None,
        time_limit: 20.0,
    };
    let mut seed = replay(snapshot, options.clone())?;
    if seed.status != "Optimal" {
        for presolve in [true, false] {
            let candidate = replay(
                snapshot,
                ReplayOptions {
                    scaling: Some(0),
                    presolve,
                    ..options.clone()
                },
            )?;
            if candidate.status == "Optimal" {
                seed = candidate;
                break;
            }
            if candidate.validation.finite
                && (
                    candidate.validation.maximum_relative_violation,
                    candidate.validation.maximum_bound_violation,
                ) < (
                    seed.validation.maximum_relative_violation,
                    seed.validation.maximum_bound_violation,
                )
            {
                seed = candidate;
            }
        }
    }
    let seed_values: Vec<_> = seed
        .values_bits
        .iter()
        .map(|x| f64::from_bits(*x))
        .collect();
    let (restricted, selected) = snapshot.restrict_support(&seed_values)?;
    let refined = replay(
        &restricted,
        ReplayOptions {
            name: "hybrid-refine".into(),
            scaling: Some(0),
            tolerance: Some(1e-10),
            small_matrix_value: Some(1e-12),
            ..options
        },
    )?;
    // Persist the actual candidate before inspecting it, including rejected or
    // singular hints. A retry may otherwise produce a different native basis.
    if let Some(source) = captured {
        let save = || -> std::result::Result<(), Box<dyn std::error::Error>> {
            let directory = source
                .parent()
                .ok_or("capture has no parent")?
                .join("candidates");
            std::fs::create_dir_all(&directory)?;
            let path = directory.join(source.file_name().ok_or("capture has no filename")?);
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            serde_json::to_writer(
                std::io::BufWriter::new(file),
                &serde_json::json!({
                    "original_snapshot":source,"restricted_snapshot":restricted,
                    "selected_columns":selected,"refined":refined,
                    "seed_status":seed.status,"seed_options":seed.options,
                }),
            )?;
            Ok(())
        };
        if let Err(error) = save() {
            eprintln!("[LP-CANDIDATE-CAPTURE-ERROR] {error}");
        }
    }
    if refined.basis_status != 0 {
        return Err("hybrid candidate has no valid basis".into());
    }
    let status = |s| match s {
        0 => Ok(exact::BasisStatus::Lower),
        1 => Ok(exact::BasisStatus::Basic),
        2 => Ok(exact::BasisStatus::Upper),
        3 => Ok(exact::BasisStatus::Zero),
        _ => Err(format!("unsupported candidate basis status {s}")),
    };
    let basis = exact::Basis {
        columns: refined
            .basis_columns
            .iter()
            .copied()
            .map(status)
            .collect::<std::result::Result<_, _>>()?,
        rows: refined
            .basis_rows
            .iter()
            .copied()
            .map(status)
            .collect::<std::result::Result<_, _>>()?,
    };
    let model = restricted.exact_model();
    let (repaired, pivots) = feasible_candidate(&model, basis)?;
    let mut values = vec![exact::Rational::zero(); snapshot.columns.len()];
    for (old, value) in selected.iter().zip(&repaired.values) {
        values[*old] = value.clone();
    }
    let solution = exact::ExactSolution { values, ..repaired };
    let certificate = snapshot.exact_model().certify(&solution)?;
    validated_result(snapshot, solution, certificate, pivots)
}

fn feasible_candidate(
    model: &exact::Model,
    basis: exact::Basis,
) -> std::result::Result<(exact::ExactSolution, usize), String> {
    let start = Instant::now();
    let initial = model.reconstruct(&basis)?;
    let initial_certificate = model.certify(&initial)?;
    eprintln!(
        "[LP-EXACT-CANDIDATE] columns={} primal={} dual={}",
        model.columns.len(),
        initial_certificate.primal_feasible,
        initial_certificate.dual_feasible
    );
    let result = if initial_certificate.primal_feasible {
        (initial, 0)
    } else {
        let result = if initial_certificate.dual_feasible {
            model.repair_dual_feasible_basis_with_observer(basis, 1000, |_| {
                start.elapsed().as_secs_f64() < 120.0
            })?
        } else {
            model.solve_dual_with_observer(10000, |_| start.elapsed().as_secs_f64() < 120.0)?
        };
        if !model.certify(&result.solution)?.primal_feasible {
            return Err(format!(
                "hybrid support repair incomplete: {:?}; restricted infeasibility={}",
                result.stop_reason,
                result.infeasibility.is_some()
            ));
        }
        (result.solution, result.pivots)
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn candidate_classification_preserves_feasibility_without_claiming_optimality() {
        let model = exact::Model {
            columns: [2.0, 1.0]
                .into_iter()
                .map(|cost| exact::Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost,
                })
                .collect(),
            rows: vec![exact::Row {
                equality: true,
                rhs: 1.0,
                terms: vec![(0, 1.0), (1, 1.0)],
            }],
            offset: 0.0,
        };
        let basis = exact::Basis {
            columns: vec![exact::BasisStatus::Basic, exact::BasisStatus::Lower],
            rows: vec![exact::BasisStatus::Upper],
        };
        let (solution, pivots) = feasible_candidate(&model, basis).unwrap();
        assert_eq!(pivots, 0);
        let certificate = model.certify(&solution).unwrap();
        assert!(certificate.primal_feasible);
        assert!(!certificate.optimal);
    }
    #[test]
    fn doubly_infeasible_candidate_is_replaced_by_a_rust_cold_start() {
        let model = exact::Model {
            columns: [1.0, 0.0, 2.0]
                .into_iter()
                .map(|cost| exact::Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost,
                })
                .collect(),
            rows: vec![exact::Row {
                equality: true,
                rhs: -1.0,
                terms: vec![(0, 1.0), (1, 1.0), (2, -1.0)],
            }],
            offset: 0.0,
        };
        let basis = exact::Basis {
            columns: vec![
                exact::BasisStatus::Basic,
                exact::BasisStatus::Lower,
                exact::BasisStatus::Lower,
            ],
            rows: vec![exact::BasisStatus::Upper],
        };
        let initial = model.certify(&model.reconstruct(&basis).unwrap()).unwrap();
        assert!(!initial.primal_feasible && !initial.dual_feasible);
        let (solution, pivots) = feasible_candidate(&model, basis).unwrap();
        assert!(pivots > 0);
        assert!(model.certify(&solution).unwrap().optimal);
    }
}
