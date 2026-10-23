//! Unverified column geometry hints; no feasibility or optimality claim.
use metatorio_solver::{lp_experiments::Snapshot, lp_rational::SolveControl};
use serde_json::json;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::{BufReader, BufWriter},
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(2..=4).contains(&args.len()) {
        return Err(
            "usage: lp_column_geometry snapshot out.json [mantissa_bits=40] [seconds=60]".into(),
        );
    }
    let bits: u32 = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(40);
    if bits > 52 {
        return Err("mantissa_bits must be between 0 and 52".into());
    }
    let seconds: f64 = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(60.0);
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("invalid time limit".into());
    }
    let begun = Instant::now();
    let deadline = begun + Duration::try_from_secs_f64(seconds)?;
    let control = SolveControl::with_deadline(deadline);
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    snapshot.check()?;
    let full = snapshot.exact_model();
    let original_nnz: usize = full
        .rows
        .iter()
        .map(|r| r.terms.iter().filter(|(_, a)| *a != 0.0).count())
        .sum();
    let reduction = full.relevance_reduction_with_control(&control)?;
    let model = &reduction.model;
    let mut columns = vec![Vec::new(); model.columns.len()];
    for (row, constraint) in model.rows.iter().enumerate() {
        if control.is_cancelled() {
            return Err("column geometry deadline exceeded".into());
        }
        for &(column, coefficient) in &constraint.terms {
            if coefficient != 0.0 {
                columns[column].push((row, coefficient));
            }
        }
    }
    let nnz: usize = columns.iter().map(Vec::len).sum();
    let mask = !((1u64 << (52 - bits)) - 1);
    let mut patterns = HashSet::new();
    // The key includes every nonzero input coefficient, even if division underflows.
    let mut groups: HashMap<Vec<(usize, u64)>, (usize, usize, f64)> = HashMap::new();
    let mut zero_columns = 0usize;
    let mut zero_representative = None;
    let mut exceptional_zero_columns = Vec::new();
    let mut normalization_underflows = 0usize;
    for (column, terms) in columns.iter().enumerate() {
        if control.is_cancelled() {
            return Err("column geometry deadline exceeded".into());
        }
        if terms.is_empty() {
            zero_columns += 1;
            let c = &model.columns[column];
            if c.cost < 0.0 || c.lower > 0.0 || c.upper < 0.0 {
                exceptional_zero_columns.push(reduction.columns[column]);
            } else if c.cost == 0.0 && zero_representative.is_none() {
                zero_representative = Some(reduction.columns[column]);
            }
            continue;
        }
        patterns.insert(terms.iter().map(|(r, _)| *r).collect::<Vec<_>>());
        let maxabs = terms.iter().map(|(_, a)| a.abs()).fold(0.0f64, f64::max);
        let costnorm = model.columns[column].cost / maxabs;
        let key = terms
            .iter()
            .map(|&(row, value)| {
                let normalized = value / maxabs;
                if normalized == 0.0 {
                    normalization_underflows += 1;
                }
                (row, normalized.to_bits() & mask)
            })
            .collect::<Vec<_>>();
        groups
            .entry(key)
            .and_modify(|entry| {
                entry.1 += 1;
                if costnorm.total_cmp(&entry.2).is_lt() {
                    entry.0 = column;
                    entry.2 = costnorm;
                }
            })
            .or_insert((column, 1, costnorm));
    }
    let mut histogram = BTreeMap::new();
    let mut selected = Vec::with_capacity(groups.len() + 1);
    let mut finite_costs = Vec::new();
    let mut nonfinite_costs = 0usize;
    for &(column, size, costnorm) in groups.values() {
        *histogram.entry(size).or_insert(0usize) += 1;
        selected.push(reduction.columns[column]);
        if costnorm.is_finite() {
            finite_costs.push(costnorm);
        } else {
            nonfinite_costs += 1;
        }
    }
    selected.extend(zero_representative);
    selected.extend(exceptional_zero_columns.iter().copied());
    selected.sort_unstable();
    selected.dedup();
    finite_costs.sort_by(f64::total_cmp);
    let report = json!({
        "snapshot": args[0], "mantissa_bits": bits,
        "method": "unverified maxabs-normalized truncated-mantissa geometry hint; groups are not proven equivalent or dominated",
        "accepted": false, "requires_original_full_model_certificate": true,
        "original": {"nrows":full.rows.len(),"ncols":full.columns.len(),"nnz":original_nnz},
        "reduced": {"nrows":model.rows.len(),"ncols":model.columns.len(),"nnz":nnz},
        "row_pattern_groups": patterns.len(), "normalized_groups": groups.len(),
        "group_size_histogram":histogram,"zero_columns":zero_columns,
        "zero_cost_zero_representative":zero_representative,
        "exceptional_zero_columns_retained":exceptional_zero_columns,
        "normalization_underflows_retained_as_entries":normalization_underflows,
        "selected_columns":selected,
        "representative_costnorm_stats":{"finite_count":finite_costs.len(),"nonfinite_count":nonfinite_costs,
            "min":finite_costs.first(),"median":finite_costs.get(finite_costs.len()/2),"max":finite_costs.last()},
        "seconds":begun.elapsed().as_secs_f64()
    });
    serde_json::to_writer_pretty(
        BufWriter::new(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&args[1])?,
        ),
        &report,
    )?;
    println!(
        "groups={} selected={} seconds={:.2}",
        groups.len(),
        report["selected_columns"].as_array().map_or(0, Vec::len),
        begun.elapsed().as_secs_f64()
    );
    Ok(())
}
