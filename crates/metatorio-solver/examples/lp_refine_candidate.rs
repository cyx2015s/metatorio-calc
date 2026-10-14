//! Exact full-model repair/optimization of a saved Rust or other basis hint.
use metatorio_solver::{
    lp_experiments::Snapshot,
    lp_rational::{Basis, DualPivotRule, ExactSolution, Refinement, SolveControl},
};
use std::{
    fs,
    io::{BufReader, BufWriter},
    time::Instant,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(4..=5).contains(&args.len()) {
        return Err(
            "usage: lp_refine_candidate snapshot.json candidate.json output.json seconds [first|largest|large-pivot|zero-large-pivot]".into(),
        );
    }
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    snapshot.check()?;
    let candidate: serde_json::Value =
        serde_json::from_reader(BufReader::new(fs::File::open(&args[1])?))?;
    let basis: Basis = serde_json::from_value(candidate["basis"].clone())?;
    let seconds: f64 = args[3].parse()?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("invalid time budget".into());
    }
    let original = snapshot.exact_model();
    let auxiliary = args.get(4).is_some_and(|s| s == "zero-large-pivot");
    let mut model = original.clone();
    if auxiliary {
        for column in &mut model.columns {
            column.cost = 0.0;
        }
        model.offset = 0.0;
    }
    let rule = match args.get(4).map(String::as_str).unwrap_or("first") {
        "first" => DualPivotRule::FirstViolation,
        "largest" => DualPivotRule::LargestViolation,
        "large-pivot" | "zero-large-pivot" => DualPivotRule::FirstViolationLargePivot,
        _ => return Err("unknown repair strategy".into()),
    };
    let start = Instant::now();
    let control = SolveControl::with_deadline(start + std::time::Duration::from_secs_f64(seconds));
    let original_duals = if auxiliary {
        Some(
            original
                .reconstruct_with_control(&basis, &control)?
                .row_duals,
        )
    } else {
        None
    };
    let result = if auxiliary {
        model
            .repair_dual_feasible_basis_with_control(
                basis,
                rule,
                1000,
                |p| {
                    eprintln!(
                        "[ZERO-REPAIR] pivots={} basis={} seconds={:.2}",
                        p.pivots,
                        p.basis_dimension,
                        start.elapsed().as_secs_f64()
                    );
                    start.elapsed().as_secs_f64() < seconds
                },
                &control,
            )
            .and_then(|r| {
                Ok(Refinement {
                    certificate: model.certify(&r.solution)?,
                    basis: r.basis,
                    solution: r.solution,
                    repair_pivots: r.pivots,
                    optimization_pivots: 0,
                    stop_reason: r.stop_reason,
                    infeasibility: r.infeasibility,
                    unbounded_ray: None,
                })
            })
    } else {
        model.refine_basis_with_rule(basis, rule, 1000, |phase, p| {
            eprintln!(
                "[REFINE] repair={phase} pivots={} basis={} seconds={:.2}",
                p.pivots,
                p.basis_dimension,
                start.elapsed().as_secs_f64()
            );
            start.elapsed().as_secs_f64() < seconds
        })
    };
    let output = match result {
        Ok(result) => {
            println!(
                "refine auxiliary={auxiliary}: optimal={} primal={} repair={} optimize={} seconds={:.2}",
                result.certificate.optimal,
                result.certificate.primal_feasible,
                result.repair_pivots,
                result.optimization_pivots,
                start.elapsed().as_secs_f64()
            );
            let values = result.solution.approximate()?;
            let combined = original_duals.map(|row_duals| ExactSolution {
                row_duals,
                ..result.solution.clone()
            });
            let original_certificate =
                combined.as_ref().map(|s| original.certify(s)).transpose()?;
            let original_gap = original_certificate
                .as_ref()
                .map(|c| c.relative_gap_upper_bound())
                .transpose()?
                .flatten();
            serde_json::json!({"model_scope":"full","auxiliary_zero_objective":auxiliary,"repair_rule":format!("{rule:?}"),"basis":result.basis,"certificate":result.certificate,
                "original_certificate":original_certificate,"original_gap_upper_bound":original_gap,
                "original_values":combined.as_ref().map(|s|s.values.iter().map(ToString::to_string).collect::<Vec<_>>()),
                "original_row_duals":combined.as_ref().map(|s|s.row_duals.iter().map(ToString::to_string).collect::<Vec<_>>()),
                "repair_pivots":result.repair_pivots,"optimization_pivots":result.optimization_pivots,"stop_reason":result.stop_reason,
                "infeasibility":result.infeasibility,"unbounded_ray":result.unbounded_ray.map(|r| r.iter().map(ToString::to_string).collect::<Vec<_>>()),
                "rounded_validation":snapshot.validate(&values),"values_bits":values.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),"elapsed_seconds":start.elapsed().as_secs_f64()})
        }
        Err(error) => {
            println!("refine error: {error}");
            serde_json::json!({"model_scope":"full","error":error,"elapsed_seconds":start.elapsed().as_secs_f64()})
        }
    };
    serde_json::to_writer(BufWriter::new(fs::File::create(&args[2])?), &output)?;
    Ok(())
}
