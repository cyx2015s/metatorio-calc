//! Inspect exact and relative residuals of a saved floating proposal, without repair.
use metatorio_solver::{
    lp_experiments::Snapshot,
    lp_rational::{Basis, BasisStatus, FloatOptions, Rational, SolveControl},
};
use num_traits::{Signed, ToPrimitive, Zero};
use std::{
    fs,
    io::BufReader,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(args.len() == 4 || (args.len() == 5 && args[4] == "--compact")) {
        return Err("usage: lp_hint_profile snapshot hint output seconds [--compact]".into());
    }
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    snapshot.check()?;
    let hint: serde_json::Value =
        serde_json::from_reader(BufReader::new(fs::File::open(&args[1])?))?;
    let mut basis: Basis = serde_json::from_value(hint["basis"].clone())?;
    let model = snapshot.exact_model();
    let start = Instant::now();
    let control =
        SolveControl::with_deadline(start + Duration::try_from_secs_f64(args[3].parse()?)?);
    let mut compact_columns = None;
    if args.len() == 5 {
        let bits: Vec<u64> = serde_json::from_value(hint["values_bits"].clone())?;
        if bits.len() != model.columns.len() {
            return Err("hint dimensions differ from snapshot".into());
        }
        let selected: Vec<_> = bits
            .iter()
            .enumerate()
            .filter_map(|(j, b)| (f64::from_bits(*b) > 1e-9).then_some(j))
            .collect();
        let mut remap = vec![None; model.columns.len()];
        for (new, old) in selected.iter().enumerate() {
            remap[*old] = Some(new);
        }
        let mut compact = model.clone();
        compact.columns = selected.iter().map(|j| model.columns[*j].clone()).collect();
        for row in &mut compact.rows {
            row.terms = row
                .terms
                .iter()
                .filter_map(|(j, a)| remap[*j].map(|new| (new, *a)))
                .collect();
        }
        let candidate = compact.float_candidate_with_options(
            FloatOptions {
                ratio_tolerance: 1e-12,
                primal_tolerance: 1e-12,
            },
            20000,
            |_| !control.is_cancelled(),
        )?;
        basis.columns.fill(BasisStatus::Lower);
        basis.rows = candidate.basis.rows;
        for (new, old) in selected.iter().enumerate() {
            basis.columns[*old] = candidate.basis.columns[new];
        }
        compact_columns = Some(selected.len());
    }
    let compact_search_seconds = start.elapsed().as_secs_f64();
    let mut point = model.reconstruct_with_control(&basis, &control)?;
    let reconstruction_seconds = start.elapsed().as_secs_f64() - compact_search_seconds;
    let certificate = model.certify(&point)?;
    let negatives: Vec<_> = point
        .values
        .iter()
        .enumerate()
        .filter(|(_, x)| x.is_negative())
        .map(|(j, x)| (j, x.to_f64()))
        .collect();
    let (scaled, scale) = model.scale_to_dual_feasible(&point)?;
    for value in &mut point.values {
        if value.is_negative() {
            *value = Rational::zero();
        }
    }
    let budget = Rational::new(1.into(), 1_000_000_000_000u64.into());
    let clipped = model.verify_relative_with_control(&point, &budget, &budget, &control)?;
    point.row_duals = scaled.row_duals;
    let scaled_clipped = model.verify_relative_with_control(&point, &budget, &budget, &control)?;
    let objective = certificate.objective.parse::<Rational>()?.to_f64();
    let record = serde_json::json!({
        "reconstruction_seconds": reconstruction_seconds, "elapsed_seconds":start.elapsed().as_secs_f64(),
        "compact_search_seconds":compact_search_seconds,"compact_columns":compact_columns,
        "basic_columns":basis.columns.iter().filter(|s| **s == BasisStatus::Basic).count(),
        "nonzero_columns":point.values.iter().filter(|v| !v.is_zero()).count(),
        "objective":objective,"certificate":certificate,"negative_values":negatives,
        "dual_scaling":scale.to_string(),"clipped_relative":clipped,"scaled_clipped_relative":scaled_clipped,
    });
    serde_json::to_writer_pretty(
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args[2])?,
        &record,
    )?;
    println!(
        "{}",
        serde_json::json!({"objective":objective,"reconstruction_seconds":reconstruction_seconds,"elapsed_seconds":start.elapsed().as_secs_f64(),"primal":certificate.primal_feasible,"dual":certificate.dual_feasible,"violated_rows":certificate.violated_rows,"violated_bounds":certificate.violated_bounds,"improving_columns":certificate.first_improving_columns.len(),"clipped_accepted":clipped.accepted,"scaled_clipped_accepted":scaled_clipped.accepted})
    );
    Ok(())
}
