//! Direct, bounded replay of constraint generation on a frozen full model.
use metatorio_solver::lp_experiments::{
    Snapshot, replay_row_frontier, replay_row_frontier_with_round_limit,
};
use std::{fs, io::BufReader, path::Path};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(3..=4).contains(&args.len()) {
        return Err("usage: lp_row_frontier snapshot output seconds [round-limit]".into());
    }
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    let output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    let result = match args.get(3) {
        Some(limit) => replay_row_frontier_with_round_limit(
            &snapshot,
            Path::new(&args[0]),
            args[2].parse()?,
            limit.parse()?,
        ),
        None => replay_row_frontier(&snapshot, Path::new(&args[0]), args[2].parse()?),
    }?;
    println!(
        "status={} seconds={}",
        result["status"], result["elapsed_seconds"]
    );
    serde_json::to_writer(output, &result)?;
    Ok(())
}
