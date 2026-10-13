//! Independently verify primal and dual witnesses obtained from different bases.
use metatorio_solver::{lp_experiments::Snapshot, lp_rational::Basis};
use std::{
    fs,
    io::{BufReader, BufWriter},
    time::Instant,
};
fn basis(path: &str) -> Result<Basis, Box<dyn std::error::Error>> {
    let value: serde_json::Value = serde_json::from_reader(BufReader::new(fs::File::open(path)?))?;
    Ok(serde_json::from_value(value["basis"].clone())?)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(4..=5).contains(&args.len()) || args.get(4).is_some_and(|s| s != "scale") {
        return Err(
            "usage: lp_cross_verify snapshot.json primal-basis.json dual-basis.json output.json [scale]"
                .into(),
        );
    }
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    let model = snapshot.exact_model();
    let start = Instant::now();
    let mut solution = model.reconstruct(&basis(&args[1])?)?;
    let dual = model.reconstruct(&basis(&args[2])?)?;
    solution.row_duals = dual.row_duals;
    let mut scale = None;
    if args.get(4).is_some() {
        let (adjusted, factor) = model.scale_to_dual_feasible(&solution)?;
        solution = adjusted;
        scale = Some(factor.to_string());
    }
    let certificate = model.certify(&solution)?;
    println!(
        "cross: primal={} dual={} optimal={} seconds={:.2}",
        certificate.primal_feasible,
        certificate.dual_feasible,
        certificate.optimal,
        start.elapsed().as_secs_f64()
    );
    let output = serde_json::json!({"model_scope":"full","primal_source":args[1],"dual_source":args[2],
        "certificate":certificate,"dual_scale":scale,"rounded_validation":snapshot.validate(&solution.approximate()?),
        "elapsed_seconds":start.elapsed().as_secs_f64()});
    serde_json::to_writer(BufWriter::new(fs::File::create(&args[3])?), &output)?;
    Ok(())
}
