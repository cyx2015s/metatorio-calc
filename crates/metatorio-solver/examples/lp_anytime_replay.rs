//! Compare optimization policies from the same original-model feasible basis.
use metatorio_solver::{
    lp_experiments::Snapshot,
    lp_rational::{Basis, FloatOptions, SolveControl},
};
use std::{
    fs,
    io::{BufReader, BufWriter},
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err(
            "usage: lp_anytime_replay snapshot seed.json|--auto output.json optimization-seconds"
                .into(),
        );
    }
    let seconds: f64 = args[3].parse()?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("invalid time budget".into());
    }
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    snapshot.check()?;
    let model = snapshot.exact_model();
    let seed_began = Instant::now();
    let basis: Basis = if args[1] == "--auto" {
        let control = SolveControl::with_deadline(Instant::now() + Duration::from_secs(30));
        let route = model.float_candidate_after_route_with_control(
            FloatOptions::default(),
            20_000,
            |_| true,
            &control,
        )?;
        let path = std::path::Path::new(&args[2]).with_extension("basis.json");
        serde_json::to_writer(
            BufWriter::new(fs::File::create(path)?),
            &serde_json::json!({"basis":route.candidate.basis}),
        )?;
        route.candidate.basis
    } else {
        let seed: serde_json::Value =
            serde_json::from_reader(BufReader::new(fs::File::open(&args[1])?))?;
        serde_json::from_value(seed["basis"].clone())?
    };
    let seed_seconds = seed_began.elapsed().as_secs_f64();
    let initial = model.certify(&model.reconstruct(&basis)?)?;
    if !initial.primal_feasible {
        return Err("seed is not original-model feasible".into());
    }
    let began = Instant::now();
    let control = SolveControl::with_deadline(
        began
            .checked_add(Duration::try_from_secs_f64(seconds)?)
            .ok_or("deadline overflow")?,
    );
    let result = model.optimize_anytime_from_basis_with_control(&basis, 100_000, &control)?;
    let elapsed_seconds = began.elapsed().as_secs_f64();
    let values = result.solution.approximate()?;
    let nonzero: Vec<_> = result.solution.values.iter().enumerate()
        .filter(|(_, x)| **x != metatorio_solver::lp_rational::Rational::from_integer(0.into()))
        .map(|(j,x)| serde_json::json!({"column":j,"label":snapshot.columns[j].label,"value":x.to_string()})).collect();
    // Diagnostic surpluses only; acceptance remains the exact certificate and
    // the existing original-model rounded validation, never this threshold.
    let surplus: Vec<_> = model.rows.iter().enumerate().filter(|(_,row)| !row.equality)
        .filter_map(|(i,row)| {
            let lhs: f64 = row.terms.iter().map(|(j,a)| a*values[*j]).sum();
            let slack = row.rhs-lhs;
            (slack > 1e-12).then(|| serde_json::json!({"row":i,"label":snapshot.rows[i].label,"approximate_surplus":slack}))
        }).collect();
    let output = serde_json::json!({"snapshot":args[0],"seed":args[1],"seed_seconds":seed_seconds,
        "optimization_seconds":seconds,"elapsed_seconds":elapsed_seconds,
        "enrichment":std::env::var("METATORIO_LP_ANYTIME_ENRICHMENT").as_deref()!=Ok("0"),
        "initial_certificate":initial,"certificate":result.certificate,"timed_out":result.timed_out,
        "pivots":result.pivots,"improving_pivots":result.improving_pivots,
        "recent_pivot_cost_changes":result.recent_pivot_cost_changes,
        "validation":snapshot.validate(&values),"nonzero":nonzero,"surplus_rows":surplus});
    serde_json::to_writer(BufWriter::new(fs::File::create(&args[2])?), &output)?;
    println!(
        "seconds={elapsed_seconds:.3} pivots={} improving={} optimal={}",
        result.pivots, result.improving_pivots, result.certificate.optimal
    );
    Ok(())
}
