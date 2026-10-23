//! Compare cold column generation with an unverified floating support hint.
use metatorio_solver::{
    lp_experiments::Snapshot,
    lp_rational::{Rational, SolveControl},
};
use std::{
    fs,
    io::BufReader,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(args.len() == 4
        || (args.len() == 5 && matches!(args[4].as_str(), "--cold" | "--warm-basis")))
    {
        return Err(
            "usage: lp_column_seed snapshot hint output seconds [--cold | --warm-basis]".into(),
        );
    }
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    snapshot.check()?;
    let cold = args.get(4).is_some_and(|s| s == "--cold");
    let hint: serde_json::Value = if cold {
        serde_json::Value::Null
    } else {
        serde_json::from_reader(BufReader::new(fs::File::open(&args[1])?))?
    };
    let bits: Vec<u64> = if cold {
        Vec::new()
    } else {
        serde_json::from_value(hint["values_bits"].clone())?
    };
    let values: Vec<_> = bits.into_iter().map(f64::from_bits).collect();
    if !cold && (values.len() != snapshot.columns.len() || values.iter().any(|v| !v.is_finite())) {
        return Err("invalid floating support hint".into());
    }
    let warm_basis = args.get(4).is_some_and(|s| s == "--warm-basis");
    let seed = (args.len() == 4).then_some(values.as_slice());
    let begun = Instant::now();
    let control =
        SolveControl::with_deadline(begun + Duration::try_from_secs_f64(args[3].parse()?)?);
    let budget = Rational::new(1.into(), 1_000_000_000_000u64.into());
    let model = snapshot.exact_model();
    let outcome = if warm_basis {
        let basis = serde_json::from_value(hint["basis"].clone())?;
        model.solve_column_generation_from_basis_with_control(&basis, &budget, &control)
    } else {
        model.solve_column_generation_with_control(seed, &budget, &control)
    };
    let record = match outcome {
        Ok(result) => {
            serde_json::json!({"accepted":result.accepted,"certificate":result.certificate,
            "infeasibility":result.infeasibility,"support_columns":result.support_columns,
            "values":result.solution.values.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "row_duals":result.solution.row_duals.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "relative_gap":result.relative_gap.map(|v|v.to_string()),"stop_reason":result.stop_reason})
        }
        Err(error) => {
            serde_json::json!({"accepted":false,"terminal_status_established":false,"error":error})
        }
    };
    let output = serde_json::json!({"seeded":seed.is_some(),"warm_basis":warm_basis,"elapsed_seconds":begun.elapsed().as_secs_f64(),"outcome":record});
    serde_json::to_writer(
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args[2])?,
        &output,
    )?;
    println!(
        "seconds={:.2} accepted={} seeded={}",
        begun.elapsed().as_secs_f64(),
        output["outcome"]["accepted"],
        seed.is_some()
    );
    Ok(())
}
