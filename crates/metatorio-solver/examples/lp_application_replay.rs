//! Replay a frozen LP through the same adapter used by production planning.
//! Select a backend in the process environment; no global mutation is needed.
use metatorio_solver::{
    lp_experiments::Snapshot,
    lp_model::{Expression, ProblemVariables, ResolutionError, variable},
    solve_lp,
};
use std::{fs, io::BufReader, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: lp_application_replay snapshot output.json".into());
    }
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    snapshot.check()?;
    if snapshot.columns.iter().any(|column| column.integer) {
        return Err("application replay supports continuous variables only".into());
    }
    let mut variables = ProblemVariables::new();
    let ids: Vec<_> = snapshot
        .columns
        .iter()
        .map(|column| {
            variables.add(
                variable()
                    .min(f64::from_bits(column.lower_bits))
                    .max(f64::from_bits(column.upper_bits))
                    .name(&column.label),
            )
        })
        .collect();
    let mut objective = Expression::from(f64::from_bits(snapshot.objective_offset_bits));
    for (column, bits) in &snapshot.objective {
        objective += f64::from_bits(*bits) * ids[*column];
    }
    let constraints = snapshot
        .rows
        .iter()
        .map(|row| {
            let mut expression = Expression::from(0.0);
            for (column, bits) in &row.terms {
                expression += f64::from_bits(*bits) * ids[*column];
            }
            let rhs = f64::from_bits(row.rhs_bits);
            if row.equality {
                expression.eq(rhs)
            } else {
                expression.leq(rhs)
            }
        })
        .collect();
    let started = Instant::now();
    let result = solve_lp(objective, constraints, variables);
    let elapsed = started.elapsed().as_secs_f64();
    let backend = std::env::var("METATORIO_LP_BACKEND").unwrap_or_else(|_| "build-default".into());
    let output = match result {
        Ok(solution) => {
            let values: Vec<_> = ids.iter().map(|id| solution.value(*id)).collect();
            let validation = snapshot.validate(&values);
            let valid = validation.finite
                && validation.maximum_relative_violation == 0.0
                && validation.maximum_bound_violation <= 1e-8;
            serde_json::json!({"backend":backend,"snapshot":args[0],"status":if valid {"solved"} else {"validation_failed"},
                "elapsed_seconds":elapsed,"columns":ids.len(),"rows":snapshot.rows.len(),"cost":solution.cost,
                "strict_bounds_met":validation.maximum_bound_violation == 0.0,
                "validation":validation,"values_bits":values.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),
                "report":{"optimization":solution.report.optimization, "exact_primal_verified":solution.report.exact_primal_verified,
                    "exact_optimality_verified":solution.report.exact_optimality_verified,
                    "relative_primal_verified":solution.report.relative_primal_verified,
                    "maximum_relative_balance_error":solution.report.maximum_relative_balance_error,
                    "balance_tolerance":solution.report.balance_tolerance,
                    "relative_duality_gap":solution.report.relative_duality_gap,
                    "certified_relative_gap":solution.report.certified_relative_gap,
                    "primal_violation":solution.report.primal_violation,"support":solution.report.variables_after}})
        }
        Err(error) => {
            serde_json::json!({"backend":backend,"snapshot":args[0],"elapsed_seconds":elapsed,
            "columns":ids.len(),"rows":snapshot.rows.len(),
            "status":match &error { ResolutionError::Infeasible=>"infeasible", ResolutionError::Unbounded=>"unbounded",ResolutionError::UnsupportedModel(_)=>"unsupported-model",_=>"rejected" },
            "error":error.to_string()})
        }
    };
    serde_json::to_writer(
        std::io::BufWriter::new(fs::File::create(&args[1])?),
        &output,
    )?;
    println!(
        "backend={backend} status={} seconds={elapsed:.4}",
        output["status"]
    );
    // Successful diagnostics may contain a rejected solve; consumers must read status.
    Ok(())
}
