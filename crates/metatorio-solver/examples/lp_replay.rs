use metatorio_solver::lp_experiments::{ReplayOptions, Snapshot, replay};
use std::{fs, io::BufReader, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.len() < 2 {
        return Err("usage: lp_replay <snapshot.json> <output-directory> [strategy-name] [time-limit-seconds]".into());
    }
    let snapshot: Snapshot =
        serde_json::from_reader(BufReader::new(fs::File::open(&arguments[0])?))?;
    let output = Path::new(&arguments[1]);
    fs::create_dir_all(output)?;
    let scale = f64::from_bits(snapshot.production_rhs_scale_bits);
    let time_limit = arguments
        .get(3)
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(60.0);
    let strategies = [
        ("default", scale, "simplex", None, true, None, None),
        ("no-scaling", scale, "simplex", Some(0), true, None, None),
        (
            "no-scaling-no-presolve",
            scale,
            "simplex",
            Some(0),
            false,
            None,
            None,
        ),
        (
            "tight",
            scale,
            "simplex",
            None,
            true,
            Some(1e-10),
            Some(1e-12),
        ),
        (
            "tight-no-presolve",
            scale,
            "simplex",
            None,
            false,
            Some(1e-10),
            Some(1e-12),
        ),
        ("ipx", scale, "ipx", None, true, Some(1e-10), Some(1e-12)),
        (
            "ipx-no-presolve",
            scale,
            "ipx",
            None,
            false,
            Some(1e-10),
            Some(1e-12),
        ),
        (
            "raw-rhs",
            1.0,
            "simplex",
            None,
            true,
            Some(1e-10),
            Some(1e-12),
        ),
    ];
    let selected = arguments.get(2);
    if selected.is_some_and(|name| name == "support-refine") {
        let options = ReplayOptions {
            name: "support-seed".into(),
            rhs_scale: scale,
            solver: "simplex".into(),
            scaling: None,
            presolve: true,
            tolerance: None,
            small_matrix_value: None,
            time_limit,
        };
        let mut seed = replay(&snapshot, options)?;
        if seed.status != "Optimal" {
            for presolve in [true, false] {
                let candidate = replay(
                    &snapshot,
                    ReplayOptions {
                        name: "support-seed-no-scaling".into(),
                        rhs_scale: scale,
                        solver: "simplex".into(),
                        scaling: Some(0),
                        presolve,
                        tolerance: None,
                        small_matrix_value: None,
                        time_limit,
                    },
                )?;
                if candidate.status == "Optimal" {
                    seed = candidate;
                    break;
                }
                if candidate.validation.finite
                    && (
                        candidate.validation.maximum_relative_violation,
                        candidate.validation.maximum_bound_violation,
                    ) < (
                        seed.validation.maximum_relative_violation,
                        seed.validation.maximum_bound_violation,
                    )
                {
                    seed = candidate;
                }
            }
        }
        let seed_values: Vec<_> = seed
            .values_bits
            .iter()
            .map(|v| f64::from_bits(*v))
            .collect();
        let (restricted, columns) = snapshot.restrict_support(&seed_values)?;
        let refined = replay(
            &restricted,
            ReplayOptions {
                name: "support-refine".into(),
                rhs_scale: scale,
                solver: "simplex".into(),
                scaling: Some(0),
                presolve: true,
                tolerance: Some(1e-10),
                small_matrix_value: Some(1e-12),
                time_limit,
            },
        )?;
        let mut original_values = vec![0.0; snapshot.columns.len()];
        for (old, value) in columns.iter().zip(&refined.values_bits) {
            original_values[*old] = f64::from_bits(*value);
        }
        let validation = snapshot.validate(&original_values);
        println!(
            "support: {} -> {} columns; {} primal={:.3e} bounds={:.3e} cost={:.12e}",
            snapshot.columns.len(),
            columns.len(),
            refined.status,
            validation.maximum_relative_violation,
            validation.maximum_bound_violation,
            validation.objective
        );
        serde_json::to_writer(
            std::io::BufWriter::new(fs::File::create(output.join("support-refine.json"))?),
            &serde_json::json!({
            "seed_status": seed.status, "seed_options": seed.options, "seed_validation": seed.validation, "selected_columns": columns, "refined": refined,
                "original_validation": validation, "original_values_bits": original_values.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                "global_optimality_verified": false,
            }),
        )?;
        return Ok(());
    }
    if selected.is_some_and(|name| !strategies.iter().any(|strategy| strategy.0 == name)) {
        return Err("unknown strategy".into());
    }
    let nonzero: Vec<_> = snapshot
        .rows
        .iter()
        .flat_map(|r| r.terms.iter().map(|(_, a)| f64::from_bits(*a).abs()))
        .filter(|v| *v > 0.0)
        .collect();
    println!(
        "model: columns={} rows={} nnz={} small<=1e-9={} scale={scale:e}",
        snapshot.columns.len(),
        snapshot.rows.len(),
        nonzero.len(),
        nonzero.iter().filter(|v| **v <= 1e-9).count()
    );
    for (name, rhs_scale, solver, scaling, presolve, tolerance, small_matrix_value) in strategies {
        if selected.is_some_and(|selected| selected != name) {
            continue;
        }
        let options = ReplayOptions {
            name: name.into(),
            rhs_scale,
            solver: solver.into(),
            scaling,
            presolve,
            tolerance,
            small_matrix_value,
            time_limit,
        };
        match replay(&snapshot, options) {
            Ok(result) => {
                println!(
                    "  repair: changes={} primal={:.3e} bounds={:.3e} cost={:.12e}",
                    result.repair.changes.len(),
                    result.repair.validation.maximum_relative_violation,
                    result.repair.validation.maximum_bound_violation,
                    result.repair.validation.objective
                );
                println!(
                    "  dual: stationarity={:.3e} gap={:.3e} sign={:.3e} finite={}",
                    result.dual_validation.maximum_relative_stationarity_error,
                    result.dual_validation.relative_objective_gap,
                    result.dual_validation.maximum_dual_sign_violation,
                    result.dual_validation.finite
                );
                println!(
                    "{name}: {} {:.2}s primal={:.3e} bounds={:.3e} finite={} cost={:.12e}",
                    result.status,
                    result.elapsed_seconds,
                    result.validation.maximum_relative_violation,
                    result.validation.maximum_bound_violation,
                    result.validation.finite,
                    result.validation.objective
                );
                for row in result.validation.worst_rows.iter().take(3) {
                    println!(
                        "  {}: abs={:.3e} rel={:.3e} lhs={:.3e} rhs={:.3e}",
                        row.label, row.absolute_violation, row.relative_violation, row.lhs, row.rhs
                    );
                }
                serde_json::to_writer(
                    std::io::BufWriter::new(fs::File::create(output.join(format!("{name}.json")))?),
                    &result,
                )?;
            }
            Err(error) => {
                println!("{name}: ERROR {error}");
                fs::write(output.join(format!("{name}.error.txt")), error)?;
            }
        }
    }
    Ok(())
}
