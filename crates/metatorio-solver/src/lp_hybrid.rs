//! Rust candidate search and exact certification for application models.
//! Optional HiGHS support/basis experiments remain behind the highs feature.
#[cfg(feature = "highs")]
use crate::lp_experiments::{ReplayOptions, replay};
use crate::{lp_experiments::Snapshot, lp_rational as exact};
#[cfg(feature = "highs")]
use num_traits::Zero;
use std::time::Instant;

#[derive(Debug)]
pub(crate) enum Failure {
    Infeasible,
    Other(String),
}
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}
impl Failure {
    pub(crate) fn resolution_error(self) -> crate::lp_model::ResolutionError {
        match self {
            Self::Infeasible => crate::lp_model::ResolutionError::Infeasible,
            Self::Other(message) => crate::lp_model::ResolutionError::Str(message),
        }
    }
}

pub(crate) struct Result {
    pub values: Vec<f64>,
    pub certificate: exact::Certificate,
    pub pivots: usize,
}

/// Pure Rust cold start; no native candidate search or fallback.
pub(crate) fn solve_rust(snapshot: &Snapshot) -> std::result::Result<Result, Failure> {
    snapshot.check()?;
    let model = snapshot.exact_model();
    let start = Instant::now();
    let result =
        model.solve_dual_with_observer(10000, |_| start.elapsed().as_secs_f64() < 120.0)?;
    let certificate = model.certify(&result.solution)?;
    if !certificate.optimal {
        if result.infeasibility.is_some() {
            return Err(Failure::Infeasible);
        }
        return Err(format!(
            "Rust cold start incomplete: {:?}; exact infeasibility={}",
            result.stop_reason,
            result.infeasibility.is_some()
        )
        .into());
    }
    validated_result(snapshot, result.solution, certificate, result.pivots).map_err(Failure::Other)
}

/// Native-free floating search, followed by exact full-model correction.
pub(crate) fn solve_rust_verified(
    snapshot: &Snapshot,
    captured: Option<&std::path::Path>,
) -> std::result::Result<Result, Failure> {
    solve_rust_proposal(snapshot, captured, false)
}
pub(crate) fn solve_rust_certified(
    snapshot: &Snapshot,
    captured: Option<&std::path::Path>,
) -> std::result::Result<Result, Failure> {
    solve_rust_proposal(snapshot, captured, true)
}
#[derive(Default)]
struct SearchWork {
    floating: usize,
    exact: usize,
    support: usize,
}
struct AttemptConfig {
    options: exact::FloatOptions,
    index: usize,
    full_refinement: bool,
}
// Independent of MCP response waiting: a slow search is unfinished work, not
// evidence that the production model cannot be solved.
const RUST_SEARCH_SECONDS: f64 = 1200.0;
fn publish_certificate(captured: Option<&std::path::Path>, attempt: usize) {
    let Some(source) = captured else {
        return;
    };
    let save = || -> std::result::Result<(), Box<dyn std::error::Error>> {
        let parent = source.parent().ok_or("capture has no parent")?;
        let name = source.file_name().ok_or("capture has no filename")?;
        let original = parent
            .join("rust-attempt-certificates")
            .join(attempt.to_string())
            .join(name);
        if !original.exists() {
            return Ok(());
        }
        let directory = parent.join("rust-certificates");
        std::fs::create_dir_all(&directory)?;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(name))?;
        std::io::copy(&mut std::fs::File::open(original)?, &mut output)?;
        Ok(())
    };
    if let Err(error) = save() {
        eprintln!("[LP-RUST-CERTIFICATE-CAPTURE-ERROR] {error}");
    }
}
fn solve_rust_proposal(
    snapshot: &Snapshot,
    captured: Option<&std::path::Path>,
    certified: bool,
) -> std::result::Result<Result, Failure> {
    snapshot.check()?;
    let start = Instant::now();
    let control = exact::SolveControl::with_deadline(
        start + std::time::Duration::from_secs_f64(RUST_SEARCH_SECONDS),
    );
    let feasibility_model = snapshot.exact_model();
    let proof = feasibility_model.prove_zero_propagation_infeasible(&control)?;
    if let Some(proof) = proof {
        if let Some(source) = captured {
            let save = || -> std::result::Result<(), Box<dyn std::error::Error>> {
                let directory = source
                    .parent()
                    .ok_or("capture has no parent")?
                    .join("rust-certificates");
                std::fs::create_dir_all(&directory)?;
                let file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(directory.join(source.file_name().ok_or("capture has no filename")?))?;
                serde_json::to_writer(
                    std::io::BufWriter::new(file),
                    &serde_json::json!({
                        "original_snapshot": source, "model_scope": "full", "accepted": false,
                    "infeasibility": proof, "phase": "feasibility-precheck",
                        "elapsed_seconds": start.elapsed().as_secs_f64()
                    }),
                )?;
                Ok(())
            };
            if let Err(error) = save() {
                eprintln!("[LP-RUST-CERTIFICATE-CAPTURE-ERROR] {error}");
            }
        }
        return Err(Failure::Infeasible);
    }
    let mut work = SearchWork::default();
    let choices = if certified {
        vec![
            (exact::FloatOptions::default(), false),
            (
                exact::FloatOptions {
                    ratio_tolerance: 1e-12,
                    primal_tolerance: 1e-12,
                },
                false,
            ),
            (
                exact::FloatOptions {
                    ratio_tolerance: 1e-14,
                    primal_tolerance: 1e-12,
                },
                false,
            ),
            (exact::FloatOptions::default(), true),
        ]
    } else {
        vec![
            (
                exact::FloatOptions {
                    ratio_tolerance: 1e-12,
                    primal_tolerance: 1e-12,
                },
                true,
            ),
            (exact::FloatOptions::default(), true),
        ]
    };
    let mut last = Err(Failure::Other("Rust search budget exhausted".into()));
    let mut last_attempt = 0;
    for (index, (options, full_refinement)) in choices.into_iter().enumerate() {
        if start.elapsed().as_secs_f64() >= RUST_SEARCH_SECONDS {
            break;
        }
        if index > 0 {
            eprintln!(
                "[LP-RUST-RETRY] attempt={} ratio={} primal={} full_refinement={full_refinement}",
                index + 1,
                options.ratio_tolerance,
                options.primal_tolerance
            );
        }
        last_attempt = index;
        last = solve_rust_proposal_attempt(
            snapshot,
            captured,
            certified,
            start,
            AttemptConfig {
                options,
                index,
                full_refinement,
            },
            &mut work,
        );
        if index == 0 && matches!(&last, Err(Failure::Other(_))) {
            let remaining = RUST_SEARCH_SECONDS - start.elapsed().as_secs_f64();
            if remaining > 0.0 {
                let deadline = Instant::now()
                    + std::time::Duration::from_secs_f64((remaining - 60.0).max(0.0));
                eprintln!(
                    "[LP-RUST-ROW-GENERATION] retrying failed hint with constraint relaxation"
                );
                match crate::lp_row_generation::solve(snapshot, captured, deadline, !certified) {
                    result @ Ok(_) | result @ Err(Failure::Infeasible) => return result,
                    Err(Failure::Other(error)) => {
                        eprintln!("[LP-RUST-ROW-GENERATION-STOP] {error}")
                    }
                }
            }
        }
        if let Err(Failure::Other(message)) = &last {
            eprintln!(
                "[LP-RUST-ATTEMPT-STOP] attempt={} seconds={:.2} reason={message}",
                index + 1,
                start.elapsed().as_secs_f64()
            );
            if let Some(source) = captured {
                let save = || -> std::result::Result<(), Box<dyn std::error::Error>> {
                    let directory = source
                        .parent()
                        .ok_or("capture has no parent")?
                        .join("rust-attempt-errors")
                        .join(index.to_string());
                    std::fs::create_dir_all(&directory)?;
                    let file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(
                            directory.join(source.file_name().ok_or("capture has no filename")?),
                        )?;
                    serde_json::to_writer(
                        std::io::BufWriter::new(file),
                        &serde_json::json!({
                            "search_attempt":index+1,"error":message,"elapsed_seconds":start.elapsed().as_secs_f64(),
                            "full_refinement":full_refinement,"ratio_tolerance":options.ratio_tolerance,
                            "primal_tolerance":options.primal_tolerance,"terminal_status_established":false
                        }),
                    )?;
                    Ok(())
                };
                if let Err(error) = save() {
                    eprintln!("[LP-RUST-ATTEMPT-ERROR-CAPTURE-ERROR] {error}");
                }
            }
        }
        if last.is_ok() || matches!(&last, Err(Failure::Infeasible)) {
            publish_certificate(captured, index);
            return last;
        }
    }
    publish_certificate(captured, last_attempt);
    last
}
fn solve_rust_proposal_attempt(
    snapshot: &Snapshot,
    captured: Option<&std::path::Path>,
    certified: bool,
    start: Instant,
    attempt: AttemptConfig,
    work: &mut SearchWork,
) -> std::result::Result<Result, Failure> {
    let options = attempt.options;
    // Healthy hints may need longer reconstruction, while later hints retain
    // time for the final complete-model repair within the shared budget.
    let deadline = if certified && !attempt.full_refinement {
        let allowance = if attempt.index == 0 { 45.0 } else { 90.0 };
        (start.elapsed().as_secs_f64() + allowance).min(RUST_SEARCH_SECONDS - 60.0)
    } else {
        RUST_SEARCH_SECONDS
    };
    let control =
        exact::SolveControl::with_deadline(start + std::time::Duration::from_secs_f64(deadline));
    snapshot.check()?;
    let model = snapshot.exact_model();
    let proposal = model.float_candidate_with_options(options, 20000, |_| {
        start.elapsed().as_secs_f64() < deadline
    })?;
    work.floating += proposal.pivots;
    if let Some(source) = captured {
        let save = || -> std::result::Result<(), Box<dyn std::error::Error>> {
            let directory =
                source
                    .parent()
                    .ok_or("capture has no parent")?
                    .join(if attempt.index == 0 {
                        "rust-candidates".to_string()
                    } else {
                        format!("rust-retries/{}", attempt.index)
                    });
            std::fs::create_dir_all(&directory)?;
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(directory.join(source.file_name().ok_or("capture has no filename")?))?;
            serde_json::to_writer(
                std::io::BufWriter::new(file),
                &serde_json::json!({
                    "search_attempt":attempt.index + 1,"original_snapshot":source,"model_scope":"full","basis":proposal.basis,
                    "pivots":proposal.pivots,"stop_reason":proposal.stop_reason,
                    "diagnostic":proposal.diagnostic,"proposal_seconds":start.elapsed().as_secs_f64(),
                    "values_bits":proposal.values.iter().map(|x|x.to_bits()).collect::<Vec<_>>(),
                    "ratio_tolerance":options.ratio_tolerance,"primal_tolerance":options.primal_tolerance,
                }),
            )?;
            Ok(())
        };
        if let Err(error) = save() {
            eprintln!("[LP-RUST-CANDIDATE-CAPTURE-ERROR] {error}");
        }
    }
    if certified {
        let allow_support = attempt.full_refinement
            || (proposal.diagnostic.is_none()
                && proposal.stop_reason
                    == "floating primal tolerances met; exact verification required");
        let budget = exact::Rational::new(1.into(), 1_000_000_000_000u64.into());
        let result = model.certify_candidate_with_control(
            &proposal,
            1000,
            &budget,
            |phase, _| {
                start.elapsed().as_secs_f64() < deadline
                    && (attempt.full_refinement || phase != "full-refine")
                    && (allow_support || !matches!(phase, "support-search" | "support-refine"))
            },
            &control,
        )?;
        work.exact += result.exact_pivots;
        work.support += result.support_search_pivots;
        if let Some(source) = captured {
            let save = || -> std::result::Result<(), Box<dyn std::error::Error>> {
                let directory = source
                    .parent()
                    .ok_or("capture has no parent")?
                    .join("rust-attempt-certificates")
                    .join(attempt.index.to_string());
                std::fs::create_dir_all(&directory)?;
                let file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(directory.join(source.file_name().ok_or("capture has no filename")?))?;
                serde_json::to_writer(
                    std::io::BufWriter::new(file),
                    &serde_json::json!({
                        "original_snapshot":source,"model_scope":"full","certificate":result.certificate,
                        "search_attempt":attempt.index + 1,"floating_attempt_pivots":work.floating,
                        "ratio_tolerance":options.ratio_tolerance,"primal_tolerance":options.primal_tolerance,
                        "accepted":result.accepted,"gap_budget":budget.to_string(),
                        "relative_gap":result.relative_gap.as_ref().map(ToString::to_string),
                        "primal_basis":result.primal_basis,"row_duals":result.solution.row_duals.iter().map(ToString::to_string).collect::<Vec<_>>(),
                        "values":result.solution.values.iter().map(ToString::to_string).collect::<Vec<_>>(),
                        "exact_pivots":result.exact_pivots,"support_columns":result.support_columns,"stop_reason":result.stop_reason,
                        "support_search_pivots":result.support_search_pivots,
                        "infeasibility":result.infeasibility,
                        "elapsed_seconds":start.elapsed().as_secs_f64()
                    }),
                )?;
                Ok(())
            };
            if let Err(error) = save() {
                eprintln!("[LP-RUST-CERTIFICATE-CAPTURE-ERROR] {error}");
            }
        }
        if result.infeasibility.is_some() {
            return Err(Failure::Infeasible);
        }
        if !result.accepted {
            return Err(format!(
                "Rust certification incomplete: {:?}; full primal={} dual={}",
                result.stop_reason,
                result.certificate.primal_feasible,
                result.certificate.dual_feasible
            )
            .into());
        }
        return validated_result(
            snapshot,
            result.solution,
            result.certificate,
            work.floating + work.support + work.exact,
        )
        .map_err(Failure::Other);
    }
    let result = model.refine_basis_with_control(
        proposal.basis,
        1000,
        |_, _| start.elapsed().as_secs_f64() < deadline,
        &control,
    )?;
    if !result.certificate.optimal {
        if result.infeasibility.is_some() {
            return Err(Failure::Infeasible);
        }
        return Err(format!(
            "Rust verified solve incomplete: {:?}; exact infeasibility={}",
            result.stop_reason,
            result.infeasibility.is_some()
        )
        .into());
    }
    validated_result(
        snapshot,
        result.solution,
        result.certificate,
        work.floating + result.repair_pivots + result.optimization_pivots,
    )
    .map_err(Failure::Other)
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

#[cfg(feature = "highs")]
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

#[cfg(any(feature = "highs", test))]
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
    fn rust_bridges_distinguish_verified_infeasibility_from_invalid_input() {
        let snapshot = Snapshot {
            format_version: 1,
            columns: vec![],
            rows: vec![crate::lp_experiments::Row {
                label: "impossible equality".into(),
                equality: true,
                rhs_bits: 1.0f64.to_bits(),
                terms: vec![],
            }],
            objective: vec![],
            objective_offset_bits: 0.0f64.to_bits(),
            production_rhs_scale_bits: 1.0f64.to_bits(),
        };
        assert!(matches!(solve_rust(&snapshot), Err(Failure::Infeasible)));
        assert!(matches!(
            solve_rust_verified(&snapshot, None),
            Err(Failure::Infeasible)
        ));
        assert!(matches!(
            solve_rust_certified(&snapshot, None),
            Err(Failure::Infeasible)
        ));
        let mut invalid = snapshot;
        invalid.format_version = 999;
        assert!(matches!(
            solve_rust_certified(&invalid, None),
            Err(Failure::Other(_))
        ));
    }
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
