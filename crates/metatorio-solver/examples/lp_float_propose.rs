//! Native-free floating basis proposal followed by independent exact checking.
use metatorio_solver::{lp_experiments::Snapshot, lp_rational::FloatOptions};
use std::{
    fs,
    io::{BufReader, BufWriter},
    time::Instant,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let hint_only = args.iter().any(|s| s == "--hint-only");
    let relevant = args.iter().any(|s| s == "--relevant");
    let reverse_columns = args.iter().any(|s| s == "--reverse-columns");
    args.retain(|s| s != "--hint-only" && s != "--relevant" && s != "--reverse-columns");
    if relevant && reverse_columns {
        return Err("relevance mapping and reversed columns must be measured separately".into());
    }
    if !(3..=5).contains(&args.len()) {
        return Err(
            "usage: lp_float_propose snapshot.json output.json seconds [ratio-tolerance] [primal-tolerance] [--hint-only] [--relevant | --reverse-columns]".into(),
        );
    }
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    snapshot.check()?;
    let seconds: f64 = args[2].parse()?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("invalid time limit".into());
    }
    let model = snapshot.exact_model();
    let start = Instant::now();
    let options = FloatOptions {
        primal_tolerance: args.get(4).map(|s| s.parse()).transpose()?.unwrap_or(1e-9),
        ratio_tolerance: args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(1e-9),
    };
    let observer = |p: metatorio_solver::lp_rational::FloatProgress| {
        if p.pivots.is_multiple_of(100) {
            eprintln!(
                "[FLOAT] pivots={} structural={} seconds={:.2}",
                p.pivots,
                p.structural_columns,
                start.elapsed().as_secs_f64()
            );
        }
        start.elapsed().as_secs_f64() < seconds
    };
    let mut search = model.clone();
    if reverse_columns {
        search.columns.reverse();
        let n = search.columns.len();
        for row in &mut search.rows {
            for (j, _) in &mut row.terms {
                *j = n - 1 - *j;
            }
        }
    }
    let reduction = if relevant {
        Some(search.relevance_reduction_with_control(
            &metatorio_solver::lp_rational::SolveControl::with_deadline(
                start + std::time::Duration::from_secs_f64(seconds),
            ),
        )?)
    } else {
        None
    };
    let mut proposal = if let Some(reduced) = &reduction {
        println!(
            "relevance: columns={} -> {} rows={} -> {}",
            model.columns.len(),
            reduced.columns.len(),
            model.rows.len(),
            reduced.rows.len()
        );
        reduced.lift_candidate(
            reduced
                .model
                .float_candidate_with_options(options, 20000, observer)?,
        )?
    } else {
        search.float_candidate_with_options(options, 20000, observer)?
    };
    if reverse_columns {
        proposal.basis.columns.reverse();
        proposal.values.reverse();
    }
    let proposal_seconds = start.elapsed().as_secs_f64();
    println!(
        "float: pivots={} seconds={:.2} reason={}",
        proposal.pivots, proposal_seconds, proposal.stop_reason
    );
    println!("diagnostic: {:?}", proposal.diagnostic);
    let mut output = serde_json::json!({"primal_tolerance":options.primal_tolerance,"ratio_tolerance":options.ratio_tolerance,"model_scope":"full","basis":proposal.basis,"pivots":proposal.pivots,"proposal_seconds":proposal_seconds,"stop_reason":proposal.stop_reason,"diagnostic":proposal.diagnostic,"values_bits":proposal.values.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),"rounded_validation":snapshot.validate(&proposal.values)});
    if let Some(reduced) = &reduction {
        output["relevance"] = serde_json::json!({"columns":reduced.columns,"rows":reduced.rows,"free_suppliers":reduced.free_suppliers});
    }
    if hint_only {
        output["exact_verification"] = "not_run".into();
        serde_json::to_writer(BufWriter::new(fs::File::create(&args[1])?), &output)?;
        return Ok(());
    }
    let start = Instant::now();
    match model.reconstruct(&proposal.basis) {
        Ok(solution) => {
            let certificate = model.certify(&solution)?;
            println!(
                "exact: primal={} dual={} optimal={} seconds={:.2}",
                certificate.primal_feasible,
                certificate.dual_feasible,
                certificate.optimal,
                start.elapsed().as_secs_f64()
            );
            output["certificate"] = serde_json::to_value(certificate)?;
            output["exact_rounded_validation"] =
                serde_json::to_value(snapshot.validate(&solution.approximate()?))?;
        }
        Err(error) => {
            println!("exact: {error}");
            output["reconstruction_error"] = error.into();
        }
    }
    output["verification_seconds"] = start.elapsed().as_secs_f64().into();
    serde_json::to_writer(BufWriter::new(fs::File::create(&args[1])?), &output)?;
    Ok(())
}
