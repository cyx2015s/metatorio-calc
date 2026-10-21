//! Replay only the bounded relative-candidate path for a saved basis hint.
use metatorio_solver::{
    lp_experiments::Snapshot,
    lp_rational::{FloatCandidate, Rational, SolveControl},
};
use std::{
    fs,
    io::BufReader,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: lp_relative_hint snapshot hint output seconds".into());
    }
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    snapshot.check()?;
    let saved: serde_json::Value =
        serde_json::from_reader(BufReader::new(fs::File::open(&args[1])?))?;
    let bits: Vec<u64> = serde_json::from_value(saved["values_bits"].clone())?;
    let hint = FloatCandidate {
        basis: serde_json::from_value(saved["basis"].clone())?,
        values: bits.into_iter().map(f64::from_bits).collect(),
        pivots: 0,
        stop_reason: "saved hint".into(),
        diagnostic: None,
    };
    let start = Instant::now();
    let control =
        SolveControl::with_deadline(start + Duration::try_from_secs_f64(args[3].parse()?)?);
    let budget = Rational::new(1.into(), 1_000_000_000_000u64.into());
    let result = snapshot
        .exact_model()
        .relative_candidate_with_control(&hint, &budget, &budget, &control)?;
    let output = match result {
        Some(result) => serde_json::json!({"accepted":true,"acceptance_mode":"relative-balance",
            "certificate":result.certificate,"relative":result.relative,"rounded_relative":result.rounded_relative,
            "values":result.solution.values.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "row_duals":result.solution.row_duals.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "support_search_pivots":result.search_pivots,"elapsed_seconds":start.elapsed().as_secs_f64()}),
        None => {
            serde_json::json!({"accepted":false,"terminal_status_established":false,"elapsed_seconds":start.elapsed().as_secs_f64()})
        }
    };
    serde_json::to_writer(
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args[2])?,
        &output,
    )?;
    println!(
        "accepted={} seconds={:.3}",
        output["accepted"],
        start.elapsed().as_secs_f64()
    );
    Ok(())
}
