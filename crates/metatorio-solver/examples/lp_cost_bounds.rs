//! Compare untrusted floating hints with/without exact incumbent-budget bounds.
use metatorio_solver::{
    lp_experiments::Snapshot,
    lp_rational::{FloatCandidate, FloatOptions, Model, Rational, SolveControl},
};
use serde_json::json;
use std::{
    fs,
    io::BufReader,
    time::{Duration, Instant},
};

fn measure(
    model: &Model,
    hint: Result<Option<FloatCandidate>, String>,
    begun: Instant,
) -> serde_json::Value {
    let hint = match hint {
        Ok(Some(hint)) => hint,
        Ok(None) => return json!({"status":"no_liftable_hint","ms":begun.elapsed().as_millis()}),
        Err(error) => return json!({"error":error,"ms":begun.elapsed().as_millis()}),
    };
    let budget = Rational::new(1.into(), 1_000_000_000_000u64.into());
    let control = SolveControl::with_deadline(Instant::now() + Duration::from_secs(10));
    match model.certify_candidate_with_control(
        &hint,
        1000,
        &budget,
        |_, _| !control.is_cancelled(),
        &control,
    ) {
        Ok(result) => {
            json!({"accepted":result.accepted,"certificate":result.certificate,"pivots":hint.pivots,"ms":begun.elapsed().as_millis(),"stop_reason":result.stop_reason})
        }
        Err(error) => json!({"error":error,"ms":begun.elapsed().as_millis()}),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 && args.len() != 4 {
        return Err("usage: lp_cost_bounds original-snapshot exact-witness new-report-path [incumbent-scale]".into());
    }
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    snapshot.check()?;
    let witness: serde_json::Value =
        serde_json::from_reader(BufReader::new(fs::File::open(&args[1])?))?;
    // Scaling must preserve exact feasibility; fixed-output targets usually
    // require scale=1, which is the default.
    let scale = args
        .get(3)
        .map(|s| s.parse::<Rational>())
        .transpose()?
        .unwrap_or_else(|| Rational::from_integer(1.into()));
    if scale <= Rational::from_integer(0.into()) {
        return Err("incumbent-scale must be positive".into());
    }
    let incumbent: Vec<Rational> = witness["values"]
        .as_array()
        .ok_or("missing exact values")?
        .iter()
        .map(|x| {
            Ok(x.as_str()
                .ok_or("non-string exact value")?
                .parse::<Rational>()?
                * &scale)
        })
        .collect::<Result<_, Box<dyn std::error::Error>>>()?;
    let model = snapshot.exact_model();
    let begun = Instant::now();
    let bounds = model.cost_bounds_with_control(
        &incumbent,
        &SolveControl::with_deadline(Instant::now() + Duration::from_secs(30)),
    )?;
    let bound_ms = begun.elapsed().as_millis();
    let begun = Instant::now();
    let baseline = model
        .float_candidate_with_options(
            FloatOptions {
                ratio_tolerance: 1e-12,
                primal_tolerance: 1e-12,
            },
            10000,
            |_| begun.elapsed() < Duration::from_secs(30),
        )
        .map(Some);
    let baseline = measure(&model, baseline, begun);
    let begun = Instant::now();
    let bounded = model.cost_bounded_float_candidate_with_control(
        &incumbent,
        &SolveControl::with_deadline(begun + Duration::from_secs(30)),
    );
    let bounded = measure(&model, bounded, begun);
    let report = json!({"snapshot":args[0],"incumbent_scale":scale.to_string(),"columns":model.columns.len(),"rows":model.rows.len(),"budget":bounds.budget.to_string(),"bound_ms":bound_ms,
        "finite_upper":bounds.upper.iter().filter(|u|u.is_some()).count(),"positive_lower":bounds.lower.iter().filter(|l|**l>Rational::from_integer(0.into())).count(),"baseline":baseline,"bounded":bounded});
    let output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    serde_json::to_writer_pretty(output, &report)?;
    println!("{report}");
    Ok(())
}
