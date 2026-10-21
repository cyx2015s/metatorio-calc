//! Exact comparison preserving physical columns, allowing auxiliary renumbering.
use metatorio_solver::lp_experiments::Snapshot;
use std::{fs::File, io::BufReader};

type Auxiliary = (u64, u64, bool, u64, Vec<(usize, u64)>);

fn read(path: &str) -> Result<Snapshot, Box<dyn std::error::Error>> {
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    snapshot.check()?;
    Ok(snapshot)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: lp_compare_snapshots old.json new.json result.json".into());
    }
    let mut a = read(&args[0])?;
    let mut b = read(&args[1])?;
    if a.format_version != b.format_version
        || a.columns.len() != b.columns.len()
        || a.rows.len() != b.rows.len()
        || a.objective_offset_bits != b.objective_offset_bits
        || a.production_rhs_scale_bits != b.production_rhs_scale_bits
    {
        return Err("model dimensions/offset/scaling differ".into());
    }
    let n = a.columns.len();
    let auxiliary: Vec<_> = a
        .columns
        .iter()
        .map(|c| {
            c.label
                .starts_with("flow:ExpandedVarId { mechanic: MechanicId(18446744073709551615),")
        })
        .collect();
    for (j, (a, b)) in a.columns.iter().zip(&b.columns).enumerate() {
        if a.label != b.label
            || a.lower_bits != b.lower_bits
            || a.upper_bits != b.upper_bits
            || a.integer != b.integer
        {
            return Err(format!("column metadata differs at {j}").into());
        }
    }
    a.objective.sort_unstable();
    b.objective.sort_unstable();
    if a.objective != b.objective {
        return Err("objective coefficients differ".into());
    }
    let mut costs = vec![0.0f64.to_bits(); n];
    let mut seen = vec![false; n];
    for &(j, bits) in &a.objective {
        if seen[j] {
            return Err("duplicate objective terms need exact aggregation".into());
        }
        seen[j] = true;
        costs[j] = bits;
    }
    let mut ids = vec![None; n];
    let mut left: Vec<Auxiliary> = Vec::new();
    for (j, c) in a.columns.iter().enumerate().filter(|(j, _)| auxiliary[*j]) {
        ids[j] = Some(left.len());
        left.push((c.lower_bits, c.upper_bits, c.integer, costs[j], Vec::new()));
    }
    let mut right = left.clone();
    let mut raw_terms_identical = true;
    for (i, (a, b)) in a.rows.iter_mut().zip(&mut b.rows).enumerate() {
        if a.label != b.label || a.equality != b.equality || a.rhs_bits != b.rhs_bits {
            return Err(format!("row metadata differs at {i}").into());
        }
        raw_terms_identical &= a.terms == b.terms;
        for &(j, bits) in &a.terms {
            if let Some(k) = ids[j] {
                left[k].4.push((i, bits));
            }
        }
        for &(j, bits) in &b.terms {
            if let Some(k) = ids[j] {
                right[k].4.push((i, bits));
            }
        }
        a.terms.retain(|(j, _)| !auxiliary[*j]);
        b.terms.retain(|(j, _)| !auxiliary[*j]);
        a.terms.sort_unstable();
        b.terms.sort_unstable();
        if a.terms != b.terms {
            return Err(format!("physical coefficients differ in row {i}: {}", a.label).into());
        }
    }
    for column in left.iter_mut().chain(&mut right) {
        column.4.sort_unstable();
    }
    left.sort_unstable();
    right.sort_unstable();
    if left != right {
        return Err("auxiliary coefficient/bound/cost multisets differ".into());
    }
    let result = serde_json::json!({"equivalent_up_to_auxiliary_permutation":true,
        "raw_terms_identical":raw_terms_identical,"physical_columns":n-left.len(),
        "auxiliary_columns":left.len(),"rows":a.rows.len(),
        "coefficient_comparison":"exact f64 bits; no numerical tolerance"});
    serde_json::to_writer(
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args[2])?,
        &result,
    )?;
    println!("{result}");
    Ok(())
}
