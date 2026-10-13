//! Rebuild a small Rust basis from a floating support, then optimize the full LP.
use metatorio_solver::{
    lp_experiments::Snapshot,
    lp_rational::{Basis, BasisStatus},
};
use std::{
    fs,
    io::{BufReader, BufWriter},
    time::Instant,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err(
            "usage: lp_rust_support snapshot.json floating-candidate.json output.json seconds"
                .into(),
        );
    }
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    let candidate: serde_json::Value =
        serde_json::from_reader(BufReader::new(fs::File::open(&args[1])?))?;
    let bits: Vec<u64> = serde_json::from_value(candidate["values_bits"].clone())?;
    let values: Vec<_> = bits.into_iter().map(f64::from_bits).collect();
    let seconds: f64 = args[3].parse()?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("invalid time budget".into());
    }
    let start = Instant::now();
    let (restricted, selected) = snapshot.restrict_support(&values)?;
    println!(
        "support columns={} full={}",
        selected.len(),
        snapshot.columns.len()
    );
    let model = restricted.exact_model();
    let proposal =
        model.float_candidate_with_observer(20000, |_| start.elapsed().as_secs_f64() < seconds)?;
    let repaired = model.refine_basis_with_observer(proposal.basis, 1000, |_, p| {
        eprintln!(
            "[SUPPORT] pivots={} basis={} seconds={:.2}",
            p.pivots,
            p.basis_dimension,
            start.elapsed().as_secs_f64()
        );
        start.elapsed().as_secs_f64() < seconds
    })?;
    let mut output = serde_json::json!({"selected_columns":selected,"restricted_certificate":repaired.certificate,
        "restricted_stop_reason":repaired.stop_reason,"restricted_basis":repaired.basis,
        "restricted_infeasibility":repaired.infeasibility,
        "restricted_seconds":start.elapsed().as_secs_f64(),
        "restricted_pivots":repaired.repair_pivots+repaired.optimization_pivots});
    // Preserve the feasible seed even if later full-model optimization is
    // interrupted; a restricted proof is explicitly kept in its own field.
    serde_json::to_writer(BufWriter::new(fs::File::create(&args[2])?), &output)?;
    if repaired.certificate.primal_feasible {
        let mut basis = Basis {
            columns: vec![BasisStatus::Lower; snapshot.columns.len()],
            rows: repaired.basis.rows,
        };
        for (old, status) in selected.iter().zip(repaired.basis.columns) {
            basis.columns[*old] = status;
        }
        // Re-enter every original column. Only this certificate can establish
        // global optimality; a restricted optimum is just a feasible seed.
        let full = snapshot
            .exact_model()
            .refine_basis_with_observer(basis, 1000, |_, p| {
                eprintln!(
                    "[FULL] pivots={} basis={} seconds={:.2}",
                    p.pivots,
                    p.basis_dimension,
                    start.elapsed().as_secs_f64()
                );
                start.elapsed().as_secs_f64() < seconds
            })?;
        let values = full.solution.approximate()?;
        println!(
            "full optimal={} primal={} pivots={} seconds={:.2}",
            full.certificate.optimal,
            full.certificate.primal_feasible,
            full.repair_pivots + full.optimization_pivots,
            start.elapsed().as_secs_f64()
        );
        output["certificate"] = serde_json::to_value(full.certificate)?;
        output["basis"] = serde_json::to_value(full.basis)?;
        output["stop_reason"] = serde_json::to_value(full.stop_reason)?;
        output["full_pivots"] = (full.repair_pivots + full.optimization_pivots).into();
        output["rounded_validation"] = serde_json::to_value(snapshot.validate(&values))?;
    } else {
        println!("restricted repair incomplete; no full-model conclusion");
    }
    output["elapsed_seconds"] = start.elapsed().as_secs_f64().into();
    serde_json::to_writer(BufWriter::new(fs::File::create(&args[2])?), &output)?;
    Ok(())
}
