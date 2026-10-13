//! Reconstruct a previously saved basis in Rust; no HiGHS solve is invoked.
use metatorio_solver::{
    lp_experiments::{ReplayResult, Snapshot},
    lp_rational::{self as exact, BasisStatus},
};
use num_traits::Zero;
use serde::Deserialize;
use std::{fs, io::BufReader, time::Instant};

#[derive(Deserialize)]
struct SupportResult {
    selected_columns: Vec<usize>,
    refined: ReplayResult,
}
fn status(value: i32) -> Result<BasisStatus, String> {
    match value {
        0 => Ok(BasisStatus::Lower),
        1 => Ok(BasisStatus::Basic),
        2 => Ok(BasisStatus::Upper),
        3 => Ok(BasisStatus::Zero),
        _ => Err(format!("unsupported basis status {value}")),
    }
}
fn model(snapshot: &Snapshot) -> exact::Model {
    let mut costs = vec![0.0; snapshot.columns.len()];
    for (i, bits) in &snapshot.objective {
        costs[*i] += f64::from_bits(*bits);
    }
    exact::Model {
        columns: snapshot
            .columns
            .iter()
            .zip(costs)
            .map(|(c, cost)| exact::Column {
                lower: f64::from_bits(c.lower_bits),
                upper: f64::from_bits(c.upper_bits),
                cost,
            })
            .collect(),
        rows: snapshot
            .rows
            .iter()
            .map(|r| exact::Row {
                equality: r.equality,
                rhs: f64::from_bits(r.rhs_bits),
                terms: r
                    .terms
                    .iter()
                    .map(|(i, a)| (*i, f64::from_bits(*a)))
                    .collect(),
            })
            .collect(),
        offset: f64::from_bits(snapshot.objective_offset_bits),
    }
}
fn run(
    snapshot: &Snapshot,
    support: SupportResult,
    optimize: bool,
    resume: Option<exact::Basis>,
) -> Result<serde_json::Value, String> {
    snapshot.check()?;
    if support.refined.basis_status != 0 {
        return Err("backend did not provide a valid basis".into());
    }
    let full = model(snapshot);
    let mut remap = vec![None; full.columns.len()];
    for (new, old) in support.selected_columns.iter().enumerate() {
        let destination = remap.get_mut(*old).ok_or("support column outside model")?;
        if destination.replace(new).is_some() {
            return Err("duplicate support column".into());
        }
    }
    let restricted = exact::Model {
        columns: support
            .selected_columns
            .iter()
            .map(|i| full.columns[*i].clone())
            .collect(),
        rows: full
            .rows
            .iter()
            .map(|r| exact::Row {
                equality: r.equality,
                rhs: r.rhs,
                terms: r
                    .terms
                    .iter()
                    .filter_map(|(i, a)| remap[*i].map(|j| (j, *a)))
                    .collect(),
            })
            .collect(),
        offset: full.offset,
    };
    let basis = exact::Basis {
        columns: support
            .refined
            .basis_columns
            .iter()
            .map(|s| status(*s))
            .collect::<Result<Vec<_>, _>>()?,
        rows: support
            .refined
            .basis_rows
            .iter()
            .map(|s| status(*s))
            .collect::<Result<Vec<_>, _>>()?,
    };
    let start = Instant::now();
    let reconstructed = restricted.reconstruct(&basis)?;
    let reconstruction_seconds = start.elapsed().as_secs_f64();
    let restricted_certificate = restricted.certify(&reconstructed)?;
    let exact_values: Vec<_> = support
        .selected_columns
        .iter()
        .zip(&reconstructed.values)
        .filter(|(_, x)| !x.is_zero())
        .map(|(i, x)| (*i, x.to_string()))
        .collect();
    let mut values = vec![num_rational::BigRational::zero(); full.columns.len()];
    for (i, x) in support.selected_columns.iter().zip(&reconstructed.values) {
        values[*i] = x.clone();
    }
    let original = exact::ExactSolution {
        values,
        ..reconstructed
    };
    if optimize {
        let mut original_basis = exact::Basis {
            columns: vec![BasisStatus::Lower; full.columns.len()],
            rows: basis.rows.clone(),
        };
        for (old, s) in support.selected_columns.iter().zip(&basis.columns) {
            original_basis.columns[*old] = *s;
        }
        if let Some(resume) = resume {
            original_basis = resume;
        }
        let start = Instant::now();
        let optimized =
            full.optimize_from_basis_with_observer(original_basis, 1000, |progress| {
                if progress.pivots % 10 == 0 {
                    eprintln!(
                        "[RUST-PIVOT] {} basis={} elapsed={:.2}s",
                        progress.pivots,
                        progress.basis_dimension,
                        start.elapsed().as_secs_f64()
                    );
                }
                start.elapsed().as_secs_f64() < 120.0
            })?;
        let seconds = start.elapsed().as_secs_f64();
        let certificate = full.certify(&optimized.solution)?;
        println!(
            "rust simplex: pivots={} exact_optimal={} unbounded={} elapsed={:.2}s",
            optimized.pivots,
            certificate.optimal,
            optimized.unbounded_ray.is_some(),
            seconds
        );
        let rounded = optimized.solution.approximate()?;
        return Ok(
            serde_json::json!({ "model_semantics":"exact binary rational values of the captured f64 model", "rust_simplex_pivots":optimized.pivots, "bland_fallback":optimized.bland_fallback, "stop_reason":optimized.stop_reason, "basis":optimized.basis, "elapsed_seconds":seconds, "full_certificate":certificate,
            "exact_nonzero_values":optimized.solution.values.iter().enumerate().filter(|(_,x)| !x.is_zero()).map(|(i,x)| (i,x.to_string())).collect::<Vec<_>>(),
            "exact_nonzero_row_duals":optimized.solution.row_duals.iter().enumerate().filter(|(_,y)| !y.is_zero()).map(|(i,y)| (i,y.to_string())).collect::<Vec<_>>(),
            "unbounded_ray":optimized.unbounded_ray.map(|ray| ray.iter().enumerate().filter(|(_,x)| !x.is_zero()).map(|(i,x)| (i,x.to_string())).collect::<Vec<_>>()),
            "rounded_validation":snapshot.validate(&rounded), "rounded_values_bits":rounded.iter().map(|x| x.to_bits()).collect::<Vec<_>>() }),
        );
    }
    let start = Instant::now();
    let full_certificate = full.certify(&original)?;
    let certification_seconds = start.elapsed().as_secs_f64();
    let approximate = original.approximate()?;
    println!(
        "exact: basis={} primal={} dual={} optimal={} rows={} bounds={} pricing={} reconstruct={:.2}s certify={:.2}s",
        original.basis_dimension,
        full_certificate.primal_feasible,
        full_certificate.dual_feasible,
        full_certificate.optimal,
        full_certificate.violated_rows,
        full_certificate.violated_bounds,
        full_certificate.violated_dual_signs,
        reconstruction_seconds,
        certification_seconds
    );
    Ok(
        serde_json::json!({ "model_semantics":"exact binary rational values of the captured f64 model", "basis_dimension":original.basis_dimension, "reconstruction_seconds":reconstruction_seconds, "certification_seconds":certification_seconds, "restricted_certificate":restricted_certificate, "full_certificate":full_certificate, "exact_nonzero_values":exact_values, "exact_nonzero_row_duals":original.row_duals.iter().enumerate().filter(|(_,y)| !y.is_zero()).map(|(i,y)| (i,y.to_string())).collect::<Vec<_>>(), "rounded_validation":snapshot.validate(&approximate), "rounded_values_bits":approximate.iter().map(|x| x.to_bits()).collect::<Vec<_>>() }),
    )
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3
        && !(args.len() == 4 && args[3] == "--solve")
        && !(args.len() == 5 && args[3] == "--resume")
    {
        return Err(
            "usage: lp_certify <original-snapshot.json> <support-refine.json> <certificate.json> [--solve | --resume checkpoint.json]"
                .into(),
        );
    }
    let snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    let support = serde_json::from_reader(BufReader::new(fs::File::open(&args[1])?))?;
    let resume = if args.len() == 5 {
        let checkpoint: serde_json::Value =
            serde_json::from_reader(BufReader::new(fs::File::open(&args[4])?))?;
        Some(serde_json::from_value(checkpoint["basis"].clone())?)
    } else {
        None
    };
    let output = match run(&snapshot, support, args.len() >= 4, resume) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("exact reconstruction rejected: {error}");
            serde_json::json!({"error":error,"optimal":false})
        }
    };
    serde_json::to_writer(
        std::io::BufWriter::new(fs::File::create(&args[2])?),
        &output,
    )?;
    Ok(())
}
