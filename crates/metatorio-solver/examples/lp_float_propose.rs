//! Native-free floating basis proposal followed by independent exact checking.
use metatorio_solver::lp_experiments::Snapshot;
use std::{
    fs,
    io::{BufReader, BufWriter},
    time::Instant,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: lp_float_propose snapshot.json output.json seconds".into());
    }
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    snapshot.check()?;
    let seconds: f64 = args[2].parse()?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("invalid time limit".into());
    }
    let model = snapshot.exact_model();
    let start = Instant::now();
    let proposal = model.float_candidate_with_observer(20000, |p| {
        if p.pivots % 100 == 0 {
            eprintln!(
                "[FLOAT] pivots={} structural={} seconds={:.2}",
                p.pivots,
                p.structural_columns,
                start.elapsed().as_secs_f64()
            );
        }
        start.elapsed().as_secs_f64() < seconds
    })?;
    let proposal_seconds = start.elapsed().as_secs_f64();
    println!(
        "float: pivots={} seconds={:.2} reason={}",
        proposal.pivots, proposal_seconds, proposal.stop_reason
    );
    println!("diagnostic: {:?}", proposal.diagnostic);
    let mut output = serde_json::json!({"model_scope":"full","basis":proposal.basis,"pivots":proposal.pivots,"proposal_seconds":proposal_seconds,"stop_reason":proposal.stop_reason,"diagnostic":proposal.diagnostic,"values_bits":proposal.values.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),"rounded_validation":snapshot.validate(&proposal.values)});
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
