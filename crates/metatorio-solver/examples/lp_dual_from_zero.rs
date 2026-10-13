//! Full-model pure Rust dual cold start. No support or candidate basis is read.
use metatorio_solver::lp_experiments::Snapshot;
use num_traits::Zero;
use std::{
    fs,
    io::{BufReader, BufWriter},
    time::Instant,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: lp_dual_from_zero snapshot.json result.json seconds".into());
    }
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    snapshot.check()?;
    let seconds: f64 = args[2].parse()?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("invalid time budget".into());
    }
    let model = snapshot.exact_model();
    let start = Instant::now();
    let result = model.solve_dual_with_observer(10000, |p| {
        if p.pivots % 10 == 0 {
            eprintln!(
                "[DUAL-ZERO] pivots={} basis={} seconds={:.2}",
                p.pivots,
                p.basis_dimension,
                start.elapsed().as_secs_f64()
            );
        }
        start.elapsed().as_secs_f64() < seconds
    });
    let output = match result {
        Ok(result) => {
            let certificate = model.certify(&result.solution)?;
            println!(
                "dual cold start: pivots={} optimal={} seconds={:.2}",
                result.pivots,
                certificate.optimal,
                start.elapsed().as_secs_f64()
            );
            serde_json::json!({"model_scope":"full", "model_semantics":"exact binary rational values of captured f64 coefficients", "pivots":result.pivots,"basis":result.basis,"stop_reason":result.stop_reason,"infeasibility":result.infeasibility,"certificate":certificate,"elapsed_seconds":start.elapsed().as_secs_f64(),
                "exact_nonzero_values":result.solution.values.iter().enumerate().filter(|(_,v)| !v.is_zero()).map(|(i,v)| (i,v.to_string())).collect::<Vec<_>>(),
                "rounded_validation":snapshot.validate(&result.solution.approximate()?)})
        }
        Err(error) => {
            serde_json::json!({"model_scope":"full","error":error,"elapsed_seconds":start.elapsed().as_secs_f64()})
        }
    };
    serde_json::to_writer(BufWriter::new(fs::File::create(&args[1])?), &output)?;
    Ok(())
}
