//! Restricted route search produces only a hint; acceptance uses the full LP.
use metatorio_solver::{
    lp_experiments::Snapshot,
    lp_rational::{
        Basis, BasisStatus, FloatCandidate, FloatOptions, Model, Rational, RouteOptions, Row,
        SolveControl,
    },
};
use std::{
    fs,
    io::{BufReader, BufWriter},
    time::{Duration, Instant},
};

fn save(path: &str, output: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    serde_json::to_writer(BufWriter::new(fs::File::create(path)?), output)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err(
            "usage: lp_route_propose snapshot.json seed.json|--auto output.json seconds".into(),
        );
    }
    let seconds: f64 = args[3].parse()?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("invalid time budget".into());
    }
    let duration = Duration::try_from_secs_f64(seconds)?;
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    snapshot.check()?;
    let n = snapshot.columns.len();
    let full = snapshot.exact_model();
    let start = Instant::now();
    let deadline = start
        .checked_add(duration)
        .ok_or("time budget exceeds Instant range")?;
    let auto = args[1] == "--auto" || args[1].starts_with("--auto:");
    let seed: serde_json::Value = if auto {
        let mut route_options = RouteOptions::default();
        let fields: Vec<_> = args[1].split(':').collect();
        if fields.len() > 4 {
            return Err("usage: --auto[:directions[:variants[:cost-weight]]]".into());
        }
        if let Some(width) = fields.get(1) {
            route_options.directions_per_row = width.parse()?;
        }
        if let Some(width) = fields.get(2) {
            route_options.variants_per_direction = width.parse()?;
        }
        if let Some(weight) = fields.get(3) {
            route_options.cost_weight = weight.parse()?;
        }
        let selected = full.route_columns_with_options_and_control(
            route_options,
            &SolveControl::with_deadline(deadline),
        )?;
        serde_json::json!({"selected_columns":selected})
    } else {
        serde_json::from_reader(BufReader::new(fs::File::open(&args[1])?))?
    };
    let values = if let Some(bits) = seed.get("values_bits") {
        let bits: Vec<u64> = serde_json::from_value(bits.clone())?;
        let values: Vec<f64> = bits.into_iter().map(f64::from_bits).collect();
        if values.len() != n || values.iter().any(|v| !v.is_finite()) {
            return Err("seed values_bits must be a finite full-dimensional vector".into());
        }
        Some(values)
    } else {
        None
    };
    let mut selected: Vec<usize> = if let Some(columns) = seed.get("selected_columns") {
        serde_json::from_value(columns.clone())?
    } else if let Some(values) = values {
        values
            .iter()
            .enumerate()
            .filter_map(|(j, v)| (*v != 0.0).then_some(j))
            .collect()
    } else {
        return Err("seed requires selected_columns or values_bits".into());
    };
    if selected.iter().any(|j| *j >= n) {
        return Err("seed column index out of range".into());
    }
    selected.sort_unstable();
    if selected.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("duplicate seed column index".into());
    }
    let mut positions = vec![None; n];
    for (j, old) in selected.iter().enumerate() {
        positions[*old] = Some(j);
    }
    let restricted = Model {
        columns: selected.iter().map(|j| full.columns[*j].clone()).collect(),
        rows: full
            .rows
            .iter()
            .map(|row| Row {
                equality: row.equality,
                rhs: row.rhs,
                terms: row
                    .terms
                    .iter()
                    .filter_map(|(j, a)| positions[*j].map(|p| (p, *a)))
                    .collect(),
            })
            .collect(),
        offset: full.offset,
    };
    restricted.check()?;
    let initial_columns_seconds = start.elapsed().as_secs_f64();
    eprintln!(
        "[ROUTE-INITIAL] automatic={auto} columns={} seconds={initial_columns_seconds:.6}",
        selected.len()
    );
    let search_duration = Duration::try_from_secs_f64(seconds * 0.8)?;
    let search_deadline = start
        .checked_add(search_duration)
        .ok_or("search budget exceeds Instant range")?;
    let search_control = SolveControl::with_deadline(search_deadline);
    let mut output = serde_json::json!({
        "original_snapshot":args[0], "seed":args[1], "model_scope":"full",
        "search_scope":"selected-route-columns", "selected_columns":selected,
        "full_columns":n, "restricted_columns":restricted.columns.len(),
        "rows":full.rows.len(), "total_budget_seconds":seconds,
        "search_budget_seconds":seconds * 0.8, "exact_verification":"pending"
    });
    output["automatic_initial_columns"] = auto.into();
    output["initial_columns_seconds"] = initial_columns_seconds.into();
    let hint = match restricted.float_candidate_after_relevance_with_control(
        FloatOptions::default(),
        20000,
        |p| {
            if p.pivots.is_multiple_of(100) {
                eprintln!(
                    "[ROUTE-FLOAT] pivots={} structural={} seconds={:.2}",
                    p.pivots,
                    p.structural_columns,
                    start.elapsed().as_secs_f64()
                );
            }
            Instant::now() < search_deadline
        },
        &search_control,
    ) {
        Ok(hint) => hint,
        Err(error) => {
            output["search_error"] = error.into();
            output["exact_verification"] = serde_json::json!({"accepted":false,"terminal_status_established":false,"error":"no floating candidate returned"});
            output["elapsed_seconds"] = start.elapsed().as_secs_f64().into();
            return save(&args[2], &output);
        }
    };
    let mut basis = Basis {
        columns: vec![BasisStatus::Lower; n],
        rows: hint.basis.rows.clone(),
    };
    let mut values = vec![0.0; n];
    for (j, old) in selected.iter().enumerate() {
        basis.columns[*old] = hint.basis.columns[j];
        values[*old] = hint.values[j];
    }
    let proposal = FloatCandidate {
        basis,
        values,
        ..hint
    };
    output["basis"] = serde_json::to_value(&proposal.basis)?;
    output["values_bits"] = serde_json::json!(
        proposal
            .values
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>()
    );
    output["pivots"] = proposal.pivots.into();
    output["stop_reason"] = proposal.stop_reason.clone().into();
    output["diagnostic"] = serde_json::to_value(&proposal.diagnostic)?;
    output["proposal_seconds"] = start.elapsed().as_secs_f64().into();
    output["rounded_validation"] = serde_json::to_value(snapshot.validate(&proposal.values))?;
    // Preserve the full-dimensional hint even if exact checking is interrupted.
    save(&args[2], &output)?;
    let verification_start = Instant::now();
    let control = SolveControl::with_deadline(deadline);
    let budget = Rational::new(1.into(), 1_000_000_000_000u64.into());
    // A route can be primal feasible without globally minimizing all columns.
    // Record those conclusions separately before testing the full optimality gap.
    output["original_basis_verification"] =
        match full.reconstruct_with_control(&proposal.basis, &control) {
            Ok(solution) => {
                let certificate = full.certify(&solution)?;
                if certificate.primal_feasible {
                    let rounded = solution.approximate()?;
                    output["verified_values_bits"] =
                        serde_json::json!(rounded.iter().map(|v| v.to_bits()).collect::<Vec<_>>());
                    output["verified_rounded_validation"] =
                        serde_json::to_value(snapshot.validate(&rounded))?;
                }
                serde_json::to_value(certificate)?
            }
            Err(error) => serde_json::json!({"error":error}),
        };
    save(&args[2], &output)?;
    output["exact_verification"] = match full
        .relative_candidate_with_control(&proposal, &budget, &budget, &control)
    {
        Ok(Some(result)) => serde_json::json!({
            "accepted":result.relative.accepted && result.rounded_relative.accepted,
            "model_scope":"full", "acceptance_mode":"relative-balance",
            "balance_budget":budget.to_string(), "gap_budget":budget.to_string(),
            "certificate":result.certificate, "relative":result.relative,
            "rounded_relative":result.rounded_relative, "basis":result.basis,
            "values":result.solution.values.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "row_duals":result.solution.row_duals.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "rounded_validation":snapshot.validate(&result.solution.approximate()?),
            "support_search_pivots":result.search_pivots
        }),
        Ok(None) => {
            serde_json::json!({"accepted":false,"terminal_status_established":false,"error":"full-model relative verification did not accept this hint"})
        }
        Err(error) => {
            serde_json::json!({"accepted":false,"terminal_status_established":false,"error":error})
        }
    };
    output["verification_seconds"] = verification_start.elapsed().as_secs_f64().into();
    output["elapsed_seconds"] = start.elapsed().as_secs_f64().into();
    save(&args[2], &output)
}
