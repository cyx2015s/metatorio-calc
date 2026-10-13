//! Replay the combined exact certification path from a saved floating hint.
use metatorio_solver::{
    lp_experiments::Snapshot,
    lp_rational::{FloatCandidate, Rational, SolveControl},
};
use std::{fs, io::BufReader, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: lp_certified_candidate snapshot candidate output seconds".into());
    }
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    snapshot.check()?;
    let hint: serde_json::Value =
        serde_json::from_reader(BufReader::new(fs::File::open(&args[1])?))?;
    let seconds: f64 = args[3].parse()?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("invalid time budget".into());
    }
    let bits: Vec<u64> = serde_json::from_value(hint["values_bits"].clone())?;
    let candidate = FloatCandidate {
        basis: serde_json::from_value(hint["basis"].clone())?,
        values: bits.into_iter().map(f64::from_bits).collect(),
        pivots: hint["pivots"].as_u64().ok_or("missing pivots")? as usize,
        stop_reason: "saved unverified hint".into(),
        diagnostic: None,
    };
    let start = Instant::now();
    let budget = Rational::new(1.into(), 1_000_000_000_000u64.into());
    let duration = std::time::Duration::try_from_secs_f64(seconds)?;
    let deadline = start.checked_add(duration).ok_or("time budget overflow")?;
    let control = SolveControl::with_deadline(deadline);
    let result = snapshot.exact_model().certify_candidate_with_control(
        &candidate,
        1000,
        &budget,
        |phase, pivots| {
            eprintln!(
                "[CERTIFIED] phase={phase} pivots={pivots} seconds={:.2}",
                start.elapsed().as_secs_f64()
            );
            start.elapsed().as_secs_f64() < seconds
        },
        &control,
    );
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            serde_json::to_writer(
                std::io::BufWriter::new(fs::File::create(&args[2])?),
                &serde_json::json!({"accepted":false,"terminal_status_established":false,
                    "error":error,"elapsed_seconds":start.elapsed().as_secs_f64()}),
            )?;
            return Err(error.into());
        }
    };
    let output = serde_json::json!({"accepted":result.accepted,"certificate":result.certificate,
        "values":result.solution.values.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "row_duals":result.solution.row_duals.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "primal_basis":result.primal_basis,"relative_gap":result.relative_gap.map(|g|g.to_string()),
        "exact_pivots":result.exact_pivots,"support_columns":result.support_columns,
        "support_search_pivots":result.support_search_pivots,"stop_reason":result.stop_reason,
        "elapsed_seconds":start.elapsed().as_secs_f64(),"gap_budget":budget.to_string(),
        "rounded_validation":snapshot.validate(&result.solution.approximate()?)});
    serde_json::to_writer(
        std::io::BufWriter::new(fs::File::create(&args[2])?),
        &output,
    )?;
    println!(
        "accepted={} primal={} dual={} optimal={} seconds={:.2}",
        result.accepted,
        result.certificate.primal_feasible,
        result.certificate.dual_feasible,
        result.certificate.optimal,
        start.elapsed().as_secs_f64()
    );
    Ok(())
}
