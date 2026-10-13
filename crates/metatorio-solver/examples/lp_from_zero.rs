//! Two-phase Rust simplex. Optional support selects columns, never a basis hint.
use metatorio_solver::{lp_experiments::Snapshot, lp_rational as exact};
use num_traits::Zero;
use serde::Deserialize;
use std::{fs, io::BufReader, time::Instant};
#[derive(Deserialize)]
struct Support {
    selected_columns: Vec<usize>,
}
fn run(
    snapshot: Snapshot,
    selected: Option<Vec<usize>>,
    seconds: f64,
) -> Result<serde_json::Value, String> {
    snapshot.check()?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("invalid time limit".into());
    }
    let full = snapshot.exact_model();
    let restricted = selected.is_some();
    let selected = selected.unwrap_or_else(|| (0..full.columns.len()).collect());
    let mut remap = vec![None; full.columns.len()];
    for (new, old) in selected.iter().enumerate() {
        let destination = remap.get_mut(*old).ok_or("support column out of bounds")?;
        if destination.replace(new).is_some() {
            return Err("duplicate support column".into());
        }
    }
    let model = exact::Model {
        columns: selected.iter().map(|i| full.columns[*i].clone()).collect(),
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
    let start = Instant::now();
    let result = model.solve_with_observer(2000, |phase1, p| {
        if p.pivots % 10 == 0 {
            eprintln!(
                "[ZERO-PIVOT] phase={} pivots={} basis={} elapsed={:.2}s",
                if phase1 { 1 } else { 2 },
                p.pivots,
                p.basis_dimension,
                start.elapsed().as_secs_f64()
            );
        }
        start.elapsed().as_secs_f64() < seconds
    })?;
    let mut output = serde_json::json!({"model_scope":if restricted {"restricted"} else {"full"},"selected_columns":selected,"columns":model.columns.len(),"rows":model.rows.len(),"phase1_pivots":result.phase1.pivots,"phase1_stop":result.phase1.stop_reason,"phase1_basis":result.phase1.basis,"elapsed_seconds":start.elapsed().as_secs_f64()});
    if let Some(proof) = result.infeasibility {
        let original = full.verify_infeasibility(&result.phase1.solution.row_duals)?;
        println!(
            "from zero: restricted={restricted} infeasible=true original_proven={}",
            original.is_some()
        );
        output["status"] = serde_json::json!("Infeasible");
        output["infeasibility_certificate"] =
            serde_json::to_value(proof).map_err(|e| e.to_string())?;
        output["original_infeasibility_certificate"] =
            serde_json::to_value(original).map_err(|e| e.to_string())?;
    } else if let Some(phase2) = result.phase2 {
        let local = model.certify(&phase2.solution)?;
        let mut values = vec![num_rational::BigRational::zero(); full.columns.len()];
        for (old, x) in selected.iter().zip(&phase2.solution.values) {
            values[*old] = x.clone();
        }
        let original = exact::ExactSolution {
            values,
            ..phase2.solution.clone()
        };
        let original_certificate = full.certify(&original)?;
        let status = if phase2.unbounded_ray.is_some() {
            "Unbounded"
        } else if local.optimal {
            "Optimal"
        } else {
            "Undetermined"
        };
        println!(
            "from zero: status={status} phase1={} phase2={} original_primal={} original_optimal={} {:.2}s",
            result.phase1.pivots,
            phase2.pivots,
            original_certificate.primal_feasible,
            original_certificate.optimal,
            start.elapsed().as_secs_f64()
        );
        output["status"] = serde_json::json!(status);
        output["unbounded_ray"] = serde_json::json!(phase2.unbounded_ray.as_ref().map(|ray| {
            selected
                .iter()
                .zip(ray)
                .filter(|(_, x)| !x.is_zero())
                .map(|(i, x)| (*i, x.to_string()))
                .collect::<Vec<_>>()
        }));
        output["phase2_pivots"] = serde_json::json!(phase2.pivots);
        output["phase2_stop"] = serde_json::json!(phase2.stop_reason);
        output["basis"] = serde_json::to_value(phase2.basis).map_err(|e| e.to_string())?;
        output["certificate"] = serde_json::to_value(local).map_err(|e| e.to_string())?;
        output["original_certificate"] =
            serde_json::to_value(original_certificate).map_err(|e| e.to_string())?;
        output["exact_nonzero_values"] = serde_json::json!(
            original
                .values
                .iter()
                .enumerate()
                .filter(|(_, x)| !x.is_zero())
                .map(|(i, x)| (i, x.to_string()))
                .collect::<Vec<_>>()
        );
        output["row_duals"] = serde_json::json!(
            original
                .row_duals
                .iter()
                .enumerate()
                .filter(|(_, x)| !x.is_zero())
                .map(|(i, x)| (i, x.to_string()))
                .collect::<Vec<_>>()
        );
        output["rounded_validation"] =
            serde_json::to_value(snapshot.validate(&original.approximate()?))
                .map_err(|e| e.to_string())?;
    } else {
        output["status"] = serde_json::json!("Undetermined");
    }
    output["elapsed_seconds"] = serde_json::json!(start.elapsed().as_secs_f64());
    Ok(output)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 2 || args.len() > 4 {
        return Err("usage: lp_from_zero <snapshot.json> <output.json> [support-refine.json | full] [seconds]".into());
    }
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    let selected = if let Some(path) = args.get(2).filter(|s| s.as_str() != "full") {
        let s: Support = serde_json::from_reader(BufReader::new(fs::File::open(path)?))?;
        Some(s.selected_columns)
    } else {
        None
    };
    let seconds = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(120.0);
    let result = match run(snapshot, selected, seconds) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("from-zero unresolved: {e}");
            serde_json::json!({"status":"Undetermined","error":e})
        }
    };
    serde_json::to_writer(
        std::io::BufWriter::new(fs::File::create(&args[1])?),
        &result,
    )?;
    Ok(())
}
