//! Check whether exact forced-zero facts remove noise from a saved basis point.
use metatorio_solver::{
    lp_experiments::Snapshot,
    lp_rational::{Basis, ExactSolution, Rational, SolveControl},
};
use num_traits::{Signed, Zero};
use std::{
    fs,
    io::BufReader,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: lp_zero_projection snapshot hint output seconds".into());
    }
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    snapshot.check()?;
    let hint: serde_json::Value =
        serde_json::from_reader(BufReader::new(fs::File::open(&args[1])?))?;
    let basis: Basis = serde_json::from_value(hint["basis"].clone())?;
    let model = snapshot.exact_model();
    let start = Instant::now();
    let control =
        SolveControl::with_deadline(start + Duration::try_from_secs_f64(args[3].parse()?)?);
    let mut solution = model.reconstruct_with_control(&basis, &control)?;
    let original_certificate = model.certify(&solution)?;
    let forced = model.zero_forced_columns_with_control(&control)?;
    let mut zeroed = Vec::new();
    for (j, x) in solution.values.iter_mut().enumerate() {
        if forced[j] && !x.is_zero() {
            zeroed.push((j, x.to_string()));
            *x = Rational::zero();
        }
        if x.is_negative() {
            *x = Rational::zero();
        }
    }
    let budget = Rational::new(1.into(), 1_000_000_000_000u64.into());
    let relative = model.verify_relative_with_control(&solution, &budget, &budget, &control)?;
    let rounded = ExactSolution {
        values: solution
            .approximate()?
            .into_iter()
            .map(|v| Rational::from_float(v).ok_or("nonfinite rounded output"))
            .collect::<Result<_, _>>()?,
        row_duals: solution.row_duals.clone(),
        basis_dimension: 0,
    };
    let rounded_relative =
        model.verify_relative_with_control(&rounded, &budget, &budget, &control)?;
    let accepted = relative.accepted && rounded_relative.accepted;
    let certificate = model.certify(&solution)?;
    let output = serde_json::json!({"accepted":accepted,"acceptance_mode":"relative-balance",
        "original_certificate":original_certificate,"certificate":certificate,"relative":relative,"rounded_relative":rounded_relative,
        "zeroed":zeroed,"elapsed_seconds":start.elapsed().as_secs_f64(),
        "values":solution.values.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "row_duals":solution.row_duals.iter().map(ToString::to_string).collect::<Vec<_>>()});
    serde_json::to_writer(
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args[2])?,
        &output,
    )?;
    println!(
        "accepted={accepted} zeroed={} seconds={:.3}",
        zeroed.len(),
        start.elapsed().as_secs_f64()
    );
    Ok(())
}
