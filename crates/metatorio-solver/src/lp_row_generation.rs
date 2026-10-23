//! Constraint generation is a relaxation of the full LP, not column pruning.
//! Every terminal outcome is certified again against the original model.
use crate::{lp_experiments::Snapshot, lp_hybrid::Failure, lp_rational as exact};
use num_traits::{Signed, Zero};
use std::time::Instant;

/// Equal columns on the current frontier are interchangeable in its relaxation.
/// Keep the cheapest (then lowest original index). New rows split groups, so
/// every previous representative remains available to the warm basis.
fn frontier_columns(
    model: &exact::Model,
    selected: &[usize],
    control: &exact::SolveControl,
) -> Result<(Vec<usize>, usize), String> {
    let mut keys = vec![Vec::<(usize, u64)>::new(); model.columns.len()];
    let mut duplicate = vec![false; model.columns.len()];
    for (position, row) in selected.iter().enumerate() {
        if control.is_cancelled() {
            return Err("frontier column compaction cancelled".into());
        }
        for &(j, coefficient) in &model.rows[*row].terms {
            if coefficient == 0.0 {
                continue;
            }
            if keys[j].last().is_some_and(|(i, _)| *i == position) {
                // Do not infer equality from unaggregated duplicate terms.
                duplicate[j] = true;
            }
            keys[j].push((position, coefficient.to_bits()));
        }
    }
    let participating = keys.iter().filter(|key| !key.is_empty()).count();
    if model
        .columns
        .iter()
        .any(|c| c.lower != 0.0 || c.upper != f64::INFINITY || c.cost < 0.0)
    {
        return Ok((
            keys.iter()
                .enumerate()
                .filter_map(|(j, k)| (!k.is_empty()).then_some(j))
                .collect(),
            participating,
        ));
    }
    let mut groups = ahash::AHashMap::<Vec<(usize, u64)>, usize>::new();
    let mut retained = Vec::new();
    for (j, key) in keys.into_iter().enumerate() {
        if key.is_empty() {
            continue;
        }
        if duplicate[j] {
            retained.push(j);
            continue;
        }
        groups
            .entry(key)
            .and_modify(|previous| {
                if model.columns[j].cost < model.columns[*previous].cost {
                    *previous = j;
                }
            })
            .or_insert(j);
    }
    retained.extend(groups.into_values());
    retained.sort_unstable();
    Ok((retained, participating))
}

pub(crate) fn solve(
    snapshot: &Snapshot,
    captured: Option<&std::path::Path>,
    deadline: Instant,
    require_optimal: bool,
) -> std::result::Result<super::lp_hybrid::Result, Failure> {
    solve_inner(snapshot, captured, deadline, require_optimal, None)
}

/// An explicit round cap is retained for bounded diagnostic replays only.
pub(crate) fn solve_with_round_limit(
    snapshot: &Snapshot,
    captured: Option<&std::path::Path>,
    deadline: Instant,
    require_optimal: bool,
    round_limit: usize,
) -> std::result::Result<super::lp_hybrid::Result, Failure> {
    solve_inner(
        snapshot,
        captured,
        deadline,
        require_optimal,
        Some(round_limit),
    )
}

fn solve_inner(
    snapshot: &Snapshot,
    captured: Option<&std::path::Path>,
    deadline: Instant,
    require_optimal: bool,
    round_limit: Option<usize>,
) -> std::result::Result<super::lp_hybrid::Result, Failure> {
    crate::control::check_cancelled()?;
    let model = snapshot.exact_model();
    let control = crate::control::exact_control(deadline);
    let mut selected: Vec<_> = model
        .rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.rhs != 0.0)
        .map(|(i, _)| i)
        .collect();
    let mut included = vec![false; model.rows.len()];
    for &i in &selected {
        included[i] = true;
    }
    let budget = if require_optimal {
        exact::Rational::zero()
    } else {
        exact::Rational::new(1.into(), 1_000_000_000_000u64.into())
    };
    let mut pivots = 0;
    let mut warm_basis: Option<exact::Basis> = None;
    // Expansion adds new rows, but some rounds only resume an unfinished
    // basis. Use the shared deadline rather than a fixed number of rounds.
    for round in 0usize.. {
        if control.is_cancelled() {
            return Err("row generation budget exhausted".to_string().into());
        }
        if round_limit.is_some_and(|limit| round >= limit) {
            return Err("row generation round limit reached".to_string().into());
        }
        // Row count alone does not bound the compressed exact basis. Let the
        // floating/dense factorization paths enforce their own resource guards
        // without rejecting every possible recovery path in advance.
        // Omit absent columns at zero, and merge exact coefficient duplicates
        // within THIS relaxation only. Every original row/column is still used
        // for terminal certification and violated rows split groups next round.
        let (columns, participating) = frontier_columns(&model, &selected, &control)?;
        if columns.len() < participating {
            eprintln!(
                "[LP-RUST-ROW-EQUIVALENT] round={} columns={} -> {}",
                round + 1,
                participating,
                columns.len()
            );
        }
        let mut positions = vec![None; model.columns.len()];
        for (j, &old) in columns.iter().enumerate() {
            positions[old] = Some(j);
        }
        let relaxed = exact::Model {
            columns: columns.iter().map(|j| model.columns[*j].clone()).collect(),
            offset: model.offset,
            rows: selected
                .iter()
                .map(|i| exact::Row {
                    equality: model.rows[*i].equality,
                    rhs: model.rows[*i].rhs,
                    terms: model.rows[*i]
                        .terms
                        .iter()
                        .filter_map(|(j, a)| {
                            if *a == 0.0 {
                                None
                            } else {
                                positions[*j].map(|p| (p, *a))
                            }
                        })
                        .collect(),
                })
                .collect(),
        };
        // Adding rows with basic slacks and previously absent columns at zero
        // preserves the old dual-feasible basis. Repair the new deficits instead
        // of discarding hundreds of exact pivots at every frontier expansion.
        let mut warm_proposal = None;
        let mut warm_verified = None;
        if let Some(previous) = &warm_basis {
            let basis = exact::Basis {
                columns: columns.iter().map(|j| previous.columns[*j]).collect(),
                rows: selected.iter().map(|i| previous.rows[*i]).collect(),
            };
            // Continue the previous basis in floating arithmetic before paying
            // for repeated large-rational LU repairs. Never replace the retained
            // exact basis with an unaccepted hint or relax the certificate gate.
            let hint_deadline = deadline.min(Instant::now() + std::time::Duration::from_secs(10));
            if let Ok(hint) = relaxed.float_candidate_with_basis(
                &basis,
                exact::FloatOptions {
                    ratio_tolerance: 1e-12,
                    primal_tolerance: 1e-12,
                },
                20000,
                |_| Instant::now() < hint_deadline && !control.is_cancelled(),
            ) {
                let hint_control = crate::control::exact_control(
                    deadline.min(Instant::now() + std::time::Duration::from_secs(5)),
                );
                if let Ok(verified) = relaxed.certify_candidate_with_control(
                    &hint,
                    1000,
                    &budget,
                    |_, _| !hint_control.is_cancelled(),
                    &hint_control,
                ) {
                    if let Some(proof) = verified.infeasibility {
                        return lift_infeasibility(&model, &selected, proof, captured);
                    }
                    if verified.accepted {
                        eprintln!(
                            "[LP-RUST-ROW-FLOAT-WARM] round={} pivots={}",
                            round + 1,
                            hint.pivots
                        );
                        pivots += hint.pivots;
                        warm_proposal = Some(exact::FloatCandidate {
                            basis: verified.primal_basis.clone(),
                            values: verified.solution.approximate()?,
                            pivots: 0,
                            stop_reason: "certified floating warm relaxation".into(),
                            diagnostic: None,
                        });
                        warm_verified = Some(verified);
                    }
                }
            }
            let warm_started = Instant::now();
            if warm_proposal.is_none()
                && let Ok(repaired) = relaxed.repair_dual_feasible_basis_with_control(
                    basis,
                    exact::DualPivotRule::FirstViolationLargePivot,
                    1000,
                    |_| warm_started.elapsed().as_secs_f64() < 20.0,
                    &control,
                )
            {
                if let Some(proof) = repaired.infeasibility {
                    return lift_infeasibility(&model, &selected, proof, captured);
                }
                if relaxed.certify(&repaired.solution)?.optimal {
                    pivots += repaired.pivots;
                    warm_proposal = Some(exact::FloatCandidate {
                        basis: repaired.basis,
                        values: repaired.solution.approximate()?,
                        pivots: 0,
                        stop_reason: "exact warm relaxation optimum".into(),
                        diagnostic: None,
                    });
                } else if repaired.stop_reason.as_deref().is_some_and(|reason| {
                    matches!(
                        reason,
                        "dual repair observer requested stop" | "dual repair pivot limit reached"
                    )
                }) {
                    // A progress stop preserves a valid dual-feasible basis.
                    // Resume it rather than discarding the completed pivots.
                    let mut resumed = exact::Basis {
                        columns: vec![exact::BasisStatus::Lower; model.columns.len()],
                        rows: vec![exact::BasisStatus::Basic; model.rows.len()],
                    };
                    for (j, &old) in columns.iter().enumerate() {
                        resumed.columns[old] = repaired.basis.columns[j];
                    }
                    for (i, &old) in selected.iter().enumerate() {
                        resumed.rows[old] = repaired.basis.rows[i];
                    }
                    eprintln!(
                        "[LP-RUST-ROW-WARM] resume pivots={} basis={}",
                        repaired.pivots, repaired.solution.basis_dimension
                    );
                    pivots += repaired.pivots;
                    warm_basis = Some(resumed);
                    continue;
                }
            }
        }
        let proposal = match warm_proposal {
            Some(p) => Ok(p),
            None => relaxed.float_candidate_with_options(
                exact::FloatOptions {
                    ratio_tolerance: 1e-12,
                    primal_tolerance: 1e-12,
                },
                20000,
                |_| !control.is_cancelled(),
            ),
        };
        let proposal_pivots = proposal.as_ref().map_or(0, |p| p.pivots);
        let refinement_deadline = deadline.min(Instant::now() + std::time::Duration::from_secs(5));
        let refinement_control = crate::control::exact_control(refinement_deadline);
        let refined = match warm_verified {
            Some(verified) => Ok(verified),
            None => proposal.and_then(|proposal| {
                relaxed.certify_candidate_with_control(
                    &proposal,
                    1000,
                    &budget,
                    |_, _| !refinement_control.is_cancelled(),
                    &refinement_control,
                )
            }),
        };
        let refined = match refined {
            Ok(r) if r.accepted || r.infeasibility.is_some() => r,
            unfinished => {
                let error = match unfinished {
                    Err(error) => error,
                    Ok(r) => format!("relaxation refinement incomplete: {:?}", r.stop_reason),
                };
                eprintln!(
                    "[LP-RUST-ROW-COLD] round={} rows={} columns={} reason={error}",
                    round + 1,
                    selected.len(),
                    columns.len()
                );
                if let Some(source) = captured {
                    let mut debug = snapshot.clone();
                    debug.columns = columns
                        .iter()
                        .map(|j| snapshot.columns[*j].clone())
                        .collect();
                    debug.rows = selected
                        .iter()
                        .enumerate()
                        .map(|(new, i)| {
                            let mut row = snapshot.rows[*i].clone();
                            row.terms = relaxed.rows[new]
                                .terms
                                .iter()
                                .map(|(j, a)| (*j, a.to_bits()))
                                .collect();
                            row
                        })
                        .collect();
                    debug.objective = relaxed
                        .columns
                        .iter()
                        .enumerate()
                        .map(|(j, c)| (j, c.cost.to_bits()))
                        .collect();
                    let dir = source.parent().unwrap().join("rust-row-relaxations");
                    let _ = std::fs::create_dir_all(&dir);
                    if let Ok(file) = std::fs::File::create(dir.join(format!(
                        "{}-round-{round}.json",
                        source.file_stem().unwrap().to_string_lossy()
                    ))) {
                        let _ = serde_json::to_writer(std::io::BufWriter::new(file), &debug);
                    }
                }
                // A failed/unsupported floating path is not an infeasibility
                // claim. In particular, many rows can still have a small exact
                // structural basis. Keep the independent all-slack recovery.
                let cold = relaxed.solve_dual_with_control(
                    exact::DualPivotRule::FirstViolationLargePivot,
                    1000,
                    |p| {
                        if p.pivots % 50 == 0 {
                            eprintln!(
                                "[LP-RUST-ROW-COLD] pivots={} basis={}",
                                p.pivots, p.basis_dimension
                            );
                        }
                        !control.is_cancelled()
                    },
                    &control,
                )?;
                if let Some(proof) = cold.infeasibility {
                    return lift_infeasibility(&model, &selected, proof, captured);
                }
                let certificate = relaxed.certify(&cold.solution)?;
                if !certificate.optimal {
                    return Err(error.into());
                }
                let candidate = exact::FloatCandidate {
                    basis: cold.basis,
                    values: cold.solution.approximate()?,
                    pivots: cold.pivots,
                    stop_reason: "exact cold relaxation optimum".into(),
                    diagnostic: None,
                };
                relaxed.certify_candidate_with_control(
                    &candidate,
                    0,
                    &budget,
                    |_, _| !control.is_cancelled(),
                    &control,
                )?
            }
        };
        pivots += proposal_pivots + refined.exact_pivots + refined.support_search_pivots;
        if let Some(proof) = refined.infeasibility {
            return lift_infeasibility(&model, &selected, proof, captured);
        }
        if !refined.accepted {
            return Err(format!(
                "row generation relaxation incomplete: {:?}",
                refined.stop_reason
            )
            .into());
        }
        let mut basis = exact::Basis {
            columns: vec![exact::BasisStatus::Lower; model.columns.len()],
            rows: vec![exact::BasisStatus::Basic; model.rows.len()],
        };
        for (j, &old) in columns.iter().enumerate() {
            basis.columns[old] = refined.primal_basis.columns[j];
        }
        for (i, &original) in selected.iter().enumerate() {
            basis.rows[original] = refined.primal_basis.rows[i];
        }
        warm_basis = Some(basis.clone());
        // Use exact signs to select new constraints. Floating tolerances never
        // erase a violated row, including deficits below normal tolerances.
        let mut violated = Vec::new();
        for (i, row) in model.rows.iter().enumerate() {
            if included[i] {
                continue;
            }
            let mut residual =
                exact::Rational::from_float(row.rhs).ok_or_else(|| "invalid rhs".to_string())?;
            for &(j, a) in &row.terms {
                if let Some(k) = positions[j]
                    && !refined.solution.values[k].is_zero()
                {
                    residual -= exact::Rational::from_float(a)
                        .ok_or_else(|| "invalid coefficient".to_string())?
                        * &refined.solution.values[k];
                }
            }
            if residual.is_negative() || row.equality && !residual.is_zero() {
                // Material contradictions often have a small proof. Adding an
                // electricity row first pulls virtually every recipe column
                // into the relaxation, obscuring that small supply subsystem.
                // Use matrix connectivity, so unnamed/replayed LPs behave the
                // same way. Dense shared-resource rows are deferred until the
                // lower-degree material balances no longer contradict a hint.
                let priority = match row.terms.len() {
                    0..=64 => 0,
                    65..=4096 => 1,
                    _ => 2,
                };
                violated.push((priority, i));
            }
        }
        // An omitted-row violation already rules out a terminal result. Avoid
        // repeatedly pricing every original column for intermediate frontiers.
        // Only a candidate satisfying all omitted rows reaches the unchanged
        // full-model primal/dual certification and final rounded-output gate.
        if violated.is_empty() {
            // Lift only terminal candidates. Missing/dropped columns are zero;
            // the residual scan above evaluates the identical sparse point
            // without cloning a million zero rational values every round.
            let mut duals = vec![exact::Rational::zero(); model.rows.len()];
            for (i, &original) in selected.iter().enumerate() {
                duals[original] = refined.solution.row_duals[i].clone();
            }
            let mut primal = vec![exact::Rational::zero(); model.columns.len()];
            for (j, &old) in columns.iter().enumerate() {
                primal[old] = refined.solution.values[j].clone();
            }
            let solution = exact::ExactSolution {
                row_duals: duals,
                values: primal,
                basis_dimension: refined.solution.basis_dimension,
            };
            let mut solution = solution;
            let mut certificate = model.certify(&solution)?;
            if !certificate.dual_feasible {
                solution = model.scale_to_dual_feasible(&solution)?.0;
                certificate = model.certify(&solution)?;
            }
            if certificate.primal_feasible
                && !certificate.relative_gap()?.is_some_and(|g| g <= budget)
            {
                // Primal and dual witnesses can be independent. Optimize the full
                // feasible primal basis without repeating Phase I to close the gap.
                eprintln!(
                    "[LP-RUST-ROW-ORIGINAL] optimizing full feasible basis={}",
                    solution.basis_dimension
                );
                if let Some(source) = captured {
                    let dir = source.parent().unwrap().join("rust-row-feasible");
                    let _ = std::fs::create_dir_all(&dir);
                    if let Ok(file) = std::fs::File::create(dir.join(source.file_name().unwrap())) {
                        let values = solution.approximate()?;
                        let _ = serde_json::to_writer(
                            std::io::BufWriter::new(file),
                            &serde_json::json!({
                                "basis":basis, "pivots":pivots,
                                "values_bits":values.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                                "accepted":false, "certificate":certificate
                            }),
                        );
                    }
                }
                let optimized = model.optimize_without_inverse_with_control(
                    basis,
                    20000,
                    |p| {
                        if p.pivots % 50 == 0 {
                            eprintln!(
                                "[LP-RUST-ROW-ORIGINAL] pivots={} basis={}",
                                p.pivots, p.basis_dimension
                            );
                        }
                        !control.is_cancelled()
                    },
                    &control,
                )?;
                pivots += optimized.pivots;
                basis = optimized.basis;
                solution = optimized.solution;
                certificate = model.certify(&solution)?;
            }
            if certificate.primal_feasible
                && certificate.dual_feasible
                && certificate.relative_gap()?.is_some_and(|g| g <= budget)
            {
                let values = solution.approximate()?;
                let rounded = snapshot.validate(&values);
                if !rounded.finite
                    || rounded.maximum_bound_violation != 0.0
                    || rounded.maximum_relative_violation != 0.0
                {
                    return Err("row generation rounded full-model check failed"
                        .to_string()
                        .into());
                }
                save(
                    captured,
                    &serde_json::json!({
                        "model_scope":"full", "accepted":true, "phase":"row-generation", "rounds":round+1,
                        "active_rows":selected.len(), "certificate":certificate,
                        "gap_budget":budget.to_string(), "relative_gap":certificate.relative_gap()?.map(|g|g.to_string()),
                        "values":solution.values.iter().map(ToString::to_string).collect::<Vec<_>>(),
                        "row_duals":solution.row_duals.iter().map(ToString::to_string).collect::<Vec<_>>(),
                        "primal_basis":basis
                    }),
                );
                return Ok(super::lp_hybrid::Result {
                    relative: None,
                    optimization: None,
                    floating_search_pivots: 0,
                    values,
                    certificate,
                    pivots,
                });
            }
        }
        violated.sort_unstable();
        let first_priority = violated.first().map(|(priority, _)| *priority);
        // After establishing the small subsystem, widen a stalled frontier
        // rather than spending many rounds on one low-degree row at a time.
        let priority_limit = first_priority.map(|p| if round >= 32 { p + 1 } else { p });
        let mut added = 0;
        for (priority, i) in violated.into_iter().take(64) {
            if Some(priority) > priority_limit {
                break;
            }
            selected.push(i);
            included[i] = true;
            added += 1;
        }
        eprintln!(
            "[LP-RUST-ROW-GENERATION] round={} active={} added={added}",
            round + 1,
            selected.len()
        );
        if added == 0 {
            return Err("row generation did not establish a full-model certificate"
                .to_string()
                .into());
        }
    }
    unreachable!()
}

fn lift_infeasibility(
    model: &exact::Model,
    selected: &[usize],
    proof: exact::InfeasibilityCertificate,
    captured: Option<&std::path::Path>,
) -> std::result::Result<super::lp_hybrid::Result, Failure> {
    let mut duals = vec![exact::Rational::zero(); model.rows.len()];
    for (i, value) in proof.row_duals {
        duals[selected[i]] = value
            .parse()
            .map_err(|_| "invalid relaxation multiplier".to_string())?;
    }
    let full = model
        .verify_infeasibility(&duals)?
        .ok_or_else(|| "relaxation proof failed full-model verification".to_string())?;
    save(
        captured,
        &serde_json::json!({ "accepted":false, "model_scope":"full",
        "phase":"row-generation", "active_rows":selected.len(), "infeasibility":full }),
    );
    Err(Failure::Infeasible)
}

pub(crate) fn save(captured: Option<&std::path::Path>, witness: &serde_json::Value) {
    let Some(source) = captured else {
        return;
    };
    let save = || -> std::result::Result<(), Box<dyn std::error::Error>> {
        let dir = source
            .parent()
            .ok_or("capture has no parent")?
            .join("rust-certificates");
        std::fs::create_dir_all(&dir)?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(source.file_name().ok_or("capture has no filename")?))?;
        serde_json::to_writer(std::io::BufWriter::new(file), witness)?;
        Ok(())
    };
    if let Err(error) = save() {
        eprintln!("[LP-RUST-CERTIFICATE-CAPTURE-ERROR] {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    type TestRow = (bool, f64, Vec<(usize, f64)>);
    fn snapshot(costs: &[f64], rows: &[TestRow]) -> Snapshot {
        Snapshot {
            format_version: 1,
            columns: costs
                .iter()
                .enumerate()
                .map(|(i, _)| crate::lp_experiments::Column {
                    label: format!("x{i}"),
                    lower_bits: 0.0f64.to_bits(),
                    upper_bits: f64::INFINITY.to_bits(),
                    integer: false,
                })
                .collect(),
            rows: rows
                .iter()
                .enumerate()
                .map(|(i, (equality, rhs, terms))| crate::lp_experiments::Row {
                    label: format!("r{i}"),
                    equality: *equality,
                    rhs_bits: rhs.to_bits(),
                    terms: terms.iter().map(|(j, a)| (*j, a.to_bits())).collect(),
                })
                .collect(),
            objective: costs
                .iter()
                .enumerate()
                .map(|(i, c)| (i, c.to_bits()))
                .collect(),
            objective_offset_bits: 0.0f64.to_bits(),
            production_rhs_scale_bits: 1.0f64.to_bits(),
        }
    }
    #[test]
    fn equivalent_frontier_columns_split_without_losing_the_warm_representative() {
        let s = snapshot(
            &[4.0, 1.0, 2.0],
            &[
                (true, 1.0, vec![(0, 1.0), (1, 1.0), (2, 1.0)]),
                (false, 0.0, vec![(1, 1.0)]),
            ],
        );
        let model = s.exact_model();
        let control = exact::SolveControl::default();
        assert_eq!(frontier_columns(&model, &[0], &control).unwrap().0, vec![1]);
        assert_eq!(
            frontier_columns(&model, &[0, 1], &control).unwrap().0,
            vec![1, 2]
        );
        let result = solve(
            &s,
            None,
            Instant::now() + std::time::Duration::from_secs(10),
            false,
        )
        .unwrap();
        assert_eq!(result.values, vec![0.0, 0.0, 1.0]);
        assert!(result.certificate.optimal);
    }
    #[test]
    fn duplicate_coefficients_and_finite_bounds_are_not_merged() {
        let s = snapshot(
            &[1.0, 1.0],
            &[(
                true,
                0.0,
                vec![(0, 1e20), (0, 1e-30), (0, -1e20), (1, 1e-30)],
            )],
        );
        assert_eq!(
            frontier_columns(&s.exact_model(), &[0], &exact::SolveControl::default())
                .unwrap()
                .0,
            vec![0, 1]
        );
        let mut model =
            snapshot(&[1.0, 2.0], &[(true, 2.0, vec![(0, 1.0), (1, 1.0)])]).exact_model();
        model.columns[0].upper = 1.0;
        assert_eq!(
            frontier_columns(&model, &[0], &exact::SolveControl::default())
                .unwrap()
                .0,
            vec![0, 1]
        );
    }
    #[test]
    fn equivalent_omitted_columns_are_covered_by_the_full_farkas_witness() {
        let s = snapshot(
            &[2.0, 1.0],
            &[
                (true, 1.0, vec![(0, 1.0), (1, 1.0)]),
                (false, 0.0, vec![(0, 1.0), (1, 1.0)]),
            ],
        );
        assert!(matches!(
            solve(
                &s,
                None,
                Instant::now() + std::time::Duration::from_secs(10),
                false
            ),
            Err(Failure::Infeasible)
        ));
    }
    #[test]
    fn demand_chain_keeps_all_suppliers_and_checks_more_than_4096_original_rows() {
        let mut rows = vec![(false, 0.0, vec![]); 4100];
        rows.extend([
            (true, 1.0, vec![(0, 1.0)]),
            (false, 0.0, vec![(0, 1.0), (1, -1.0)]),
            (false, 0.0, vec![(1, 1.0), (2, -1.0)]),
        ]);
        let s = snapshot(&[1.0, 2.0, 3.0], &rows);
        let result = solve(
            &s,
            None,
            Instant::now() + std::time::Duration::from_secs(10),
            false,
        )
        .unwrap();
        assert_eq!(result.values, vec![1.0, 1.0, 1.0]);
        assert!(result.certificate.optimal);
        assert_eq!(s.validate(&result.values).maximum_relative_violation, 0.0);
    }
    #[test]
    fn lossy_supply_cycle_requires_full_model_infeasibility() {
        let s = snapshot(
            &[1.0, 1.0],
            &[
                (true, 1.0, vec![(0, 1.0)]),
                (false, 0.0, vec![(0, 1.0), (1, -1.0)]),
                (false, 0.0, vec![(1, 2.0), (0, -1.0)]),
            ],
        );
        assert!(matches!(
            solve(
                &s,
                None,
                Instant::now() + std::time::Duration::from_secs(10),
                false,
            ),
            Err(Failure::Infeasible)
        ));
    }
    #[test]
    fn verified_recovery_requires_zero_original_objective_gap() {
        let s = snapshot(&[1.0, 0.3], &[(false, -1.0, vec![(0, -7.0), (1, -2.1)])]);
        let approximate = solve(
            &s,
            None,
            Instant::now() + std::time::Duration::from_secs(10),
            false,
        )
        .unwrap();
        assert!(approximate.certificate.primal_feasible && approximate.certificate.dual_feasible);
        assert!(!approximate.certificate.optimal);
        assert!(
            approximate
                .certificate
                .relative_gap()
                .unwrap()
                .unwrap()
                .is_positive()
        );
        let result = solve(
            &s,
            None,
            Instant::now() + std::time::Duration::from_secs(10),
            true,
        )
        .unwrap();
        assert!(result.certificate.optimal);
        assert_eq!(
            result.certificate.relative_gap().unwrap().unwrap(),
            exact::Rational::zero()
        );
    }
    #[test]
    fn cancellation_never_becomes_an_infeasibility_conclusion() {
        let s = snapshot(&[1.0], &[(true, 1.0, vec![(0, 1.0)])]);
        assert!(matches!(
            solve(&s, None, Instant::now(), false),
            Err(Failure::Other(_))
        ));
    }
    #[test]
    fn more_than_4096_active_rows_can_have_a_one_dimensional_basis() {
        // Each inequality -x <= -1 requires x >= 1.
        let rows = vec![(false, -1.0, vec![(0, -1.0)]); 4097];
        let s = snapshot(&[1.0], &rows);
        let result = solve(
            &s,
            None,
            Instant::now() + std::time::Duration::from_secs(15),
            true,
        )
        .unwrap();
        assert!(result.certificate.optimal);
        assert_eq!(result.values, vec![1.0]);
    }

    #[test]
    fn production_frontier_can_advance_beyond_256_rounds() {
        let count = 258;
        let mut rows = vec![(true, 1.0, vec![(0, 1.0)])];
        for j in 0..count - 1 {
            rows.push((false, 0.0, vec![(j, 1.0), (j + 1, -1.0)]));
        }
        let s = snapshot(&vec![1.0; count], &rows);
        let result = solve(
            &s,
            None,
            Instant::now() + std::time::Duration::from_secs(90),
            true,
        )
        .unwrap();
        assert!(result.certificate.optimal);
        assert_eq!(result.values, vec![1.0; count]);
    }

    #[test]
    fn round_limit_stops_without_an_infeasibility_claim() {
        let s = snapshot(
            &[1.0, 2.0],
            &[
                (true, 1.0, vec![(0, 1.0)]),
                (false, 0.0, vec![(0, 1.0), (1, -1.0)]),
            ],
        );
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        assert!(matches!(
            solve_with_round_limit(&s, None, deadline, false, 1),
            Err(Failure::Other(message)) if message == "row generation round limit reached"
        ));
        let solved = solve_with_round_limit(&s, None, deadline, false, 2).unwrap();
        assert!(solved.certificate.optimal);
        assert_eq!(solved.values, vec![1.0, 1.0]);
    }
}
