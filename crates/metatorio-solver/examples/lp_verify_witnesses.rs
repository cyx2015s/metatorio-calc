//! Recheck every captured original model using stored exact witnesses only.
//! Does not trust saved status, gap, basis, or any floating search result.
use metatorio_solver::{
    lp_experiments::Snapshot,
    lp_rational::{ExactSolution, Rational},
};
use std::{fs, io::BufReader, path::PathBuf};
fn vector(value: &serde_json::Value) -> Result<Vec<Rational>, Box<dyn std::error::Error>> {
    value
        .as_array()
        .ok_or("missing exact vector")?
        .iter()
        .map(|v| {
            Ok(v.as_str()
                .ok_or("non-string rational")?
                .parse::<Rational>()?)
        })
        .collect()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let allow_infeasible = args.len() == 2 && args[1] == "--allow-infeasible";
    if args.len() != 1 && !allow_infeasible {
        return Err("usage: lp_verify_witnesses capture-directory [--allow-infeasible]".into());
    }
    let directory = PathBuf::from(&args[0]);
    let budget = Rational::new(1.into(), 1_000_000_000_000u64.into());
    let mut count = 0;
    let mut optima = 0;
    let mut infeasible = 0;
    let mut maximum_gap = 0.0f64;
    let mut relative_models = 0;
    let mut maximum_balance_error = 0.0f64;
    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let text = name.to_string_lossy();
        if !text.starts_with("lp-") || !text.ends_with(".json") || !entry.file_type()?.is_file() {
            continue;
        }
        let snapshot: Snapshot =
            serde_json::from_reader(BufReader::new(fs::File::open(entry.path())?))?;
        snapshot.check()?;
        let witness: serde_json::Value = serde_json::from_reader(BufReader::new(fs::File::open(
            directory.join("rust-certificates").join(&name),
        )?))?;
        if allow_infeasible && !witness["infeasibility"].is_null() {
            let proof = &witness["infeasibility"];
            let mut multipliers = vec![Rational::from_integer(0.into()); snapshot.rows.len()];
            let mut seen = std::collections::HashSet::new();
            let entries: Vec<(usize, String)> = serde_json::from_value(proof["row_duals"].clone())?;
            for (row, value) in entries {
                if row >= multipliers.len() || !seen.insert(row) {
                    return Err(format!("invalid Farkas row index: {text}").into());
                }
                multipliers[row] = value.parse()?;
            }
            let verified = snapshot
                .exact_model()
                .verify_infeasibility(&multipliers)?
                .ok_or_else(|| format!("invalid original-model Farkas witness: {text}"))?;
            if witness["accepted"] != false || serde_json::to_value(&verified)? != *proof {
                return Err(format!("stored infeasibility proof mismatch: {text}").into());
            }
            count += 1;
            infeasible += 1;
            continue;
        }
        let solution = ExactSolution {
            values: vector(&witness["values"])?,
            row_duals: vector(&witness["row_duals"])?,
            basis_dimension: 0,
        };
        let certificate = snapshot.exact_model().certify(&solution)?;
        if witness["acceptance_mode"] == "relative-balance" {
            let model = snapshot.exact_model();
            let control = Default::default();
            let verified =
                model.verify_relative_with_control(&solution, &budget, &budget, &control)?;
            let rounded_solution = ExactSolution {
                values: solution
                    .approximate()?
                    .into_iter()
                    .map(|value| Rational::from_float(value).ok_or("nonfinite rounded value"))
                    .collect::<Result<_, _>>()?,
                row_duals: solution.row_duals.clone(),
                basis_dimension: 0,
            };
            let rounded = model.verify_relative_with_control(
                &rounded_solution,
                &budget,
                &budget,
                &control,
            )?;
            if !verified.accepted
                || !rounded.accepted
                || witness["accepted"] != true
                || witness["certificate"] != serde_json::to_value(&certificate)?
                || witness["relative"] != serde_json::to_value(&verified)?
                || witness["rounded_relative"] != serde_json::to_value(&rounded)?
            {
                return Err(format!("invalid relative-balance witness: {text}").into());
            }
            maximum_balance_error = maximum_balance_error
                .max(verified.balance_error_upper_bound()?)
                .max(rounded.balance_error_upper_bound()?);
            maximum_gap = maximum_gap
                .max(
                    verified
                        .duality_gap_upper_bound()?
                        .ok_or("missing original duality bound")?,
                )
                .max(
                    rounded
                        .duality_gap_upper_bound()?
                        .ok_or("missing rounded duality bound")?,
                );
            relative_models += usize::from(!certificate.primal_feasible);
            optima += usize::from(certificate.optimal);
            count += 1;
            continue;
        }
        let gap = certificate
            .relative_gap()?
            .ok_or("witness does not establish full feasibility")?;
        if gap > budget {
            return Err(format!("objective-gap budget failed: {text}").into());
        }
        // An exact rational certificate does not by itself certify the f64
        // output. Apply the same no-absolute-floor row budget on this path too.
        let rounded_solution = ExactSolution {
            values: solution
                .approximate()?
                .into_iter()
                .map(|value| Rational::from_float(value).ok_or("nonfinite rounded value"))
                .collect::<Result<_, _>>()?,
            row_duals: solution.row_duals.clone(),
            basis_dimension: 0,
        };
        let pure_relative = snapshot.exact_model().verify_relative_with_control(
            &rounded_solution,
            &budget,
            &budget,
            &Default::default(),
        )?;
        if !pure_relative.accepted {
            return Err(format!("rounded full-model pure-relative budget failed: {text}").into());
        }
        maximum_balance_error =
            maximum_balance_error.max(pure_relative.balance_error_upper_bound()?);
        maximum_gap = maximum_gap.max(
            pure_relative
                .duality_gap_upper_bound()?
                .ok_or("missing rounded duality comparison")?,
        );
        let rounded = snapshot.validate(&solution.approximate()?);
        if !rounded.finite
            || rounded.maximum_relative_violation != 0.0
            || rounded.maximum_bound_violation != 0.0
        {
            return Err(format!("rounded witness exceeds budgets: {text}").into());
        }
        if witness["accepted"] != true
            || witness["certificate"] != serde_json::to_value(&certificate)?
        {
            return Err(format!("stored status/certificate mismatch: {text}").into());
        }
        maximum_gap = maximum_gap.max(
            certificate
                .relative_gap_upper_bound()?
                .ok_or("missing reporting bound")?,
        );
        count += 1;
        optima += usize::from(certificate.optimal);
    }
    if count == 0 {
        return Err("capture directory has no LP snapshots".into());
    }
    println!(
        "{}",
        serde_json::json!({"verified_models":count,"exact_optima":optima,"gap_budget":budget.to_string(),
        "verified_infeasible_models":infeasible,"verified_feasible_models":count-infeasible-relative_models,
        "verified_relative_feasible_models":relative_models,"maximum_relative_balance_error":maximum_balance_error,
        "maximum_relative_gap_upper_bound":maximum_gap,"all_primal_and_dual_feasible":infeasible==0 && relative_models==0,"all_within_verified_budgets":true,"all_rounded_checks_passed":true})
    );
    Ok(())
}
