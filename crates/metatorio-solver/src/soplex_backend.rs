//! Optional SoPlex backend through the patched vendored `soplex-sys`.
//!
//! The safe binding's add-column API is dense, so the production model is
//! written as a sparse MPS file and read through SoPlex's file reader. The
//! vendored build enables Boost.Multiprecision and this adapter requests
//! rational read/solve/check modes with zero tolerances. SoPlex certifies the
//! MPS model exactly, then returns the primal values as `f64` for the LP model.
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::lp_model::{ResolutionError, VariableDefinition};

use super::ParsedRow;

static SERIAL: AtomicU64 = AtomicU64::new(0);

struct TempMps(PathBuf);
impl Drop for TempMps {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn time_limit_seconds() -> f64 {
    std::env::var("METATORIO_LP_SEARCH_SECONDS")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .unwrap_or(900.0)
}

fn write_number(out: &mut String, value: f64) -> Result<(), ResolutionError> {
    use std::fmt::Write as _;
    write!(out, "{value:.17e}").map_err(|error| ResolutionError::Str(error.to_string()))
}

fn write_mps(
    path: &Path,
    defs: &[VariableDefinition],
    objective: &[(usize, f64)],
    rows: &[ParsedRow],
) -> Result<(), ResolutionError> {
    use std::fmt::Write as _;

    let mut costs = vec![0.0f64; defs.len()];
    for &(index, cost) in objective {
        costs[index] += cost;
    }
    let mut columns: Vec<Vec<(usize, f64)>> = vec![Vec::new(); defs.len()];
    for (row_index, row) in rows.iter().enumerate() {
        for &(column, coefficient) in &row.terms {
            columns[column].push((row_index, coefficient));
        }
    }

    let mut out = String::with_capacity(64 * 1024 * 1024);
    writeln!(out, "NAME          METATORIO").map_err(|e| ResolutionError::Str(e.to_string()))?;
    writeln!(out, "ROWS").map_err(|e| ResolutionError::Str(e.to_string()))?;
    writeln!(out, " N  OBJ").map_err(|e| ResolutionError::Str(e.to_string()))?;
    for (index, row) in rows.iter().enumerate() {
        let kind = if row.is_equality { "E" } else { "L" };
        writeln!(out, " {kind}  R{index}").map_err(|e| ResolutionError::Str(e.to_string()))?;
    }
    writeln!(out, "COLUMNS").map_err(|e| ResolutionError::Str(e.to_string()))?;
    for (column, (definition, cost)) in defs.iter().zip(costs).enumerate() {
        let _ = definition;
        write!(out, "    X{column}  OBJ  ").map_err(|e| ResolutionError::Str(e.to_string()))?;
        write_number(&mut out, cost)?;
        out.push('\n');
        for &(row, coefficient) in &columns[column] {
            write!(out, "    X{column}  R{row}  ")
                .map_err(|e| ResolutionError::Str(e.to_string()))?;
            write_number(&mut out, coefficient)?;
            out.push('\n');
        }
    }
    if rows.iter().any(|row| row.rhs != 0.0) {
        writeln!(out, "RHS").map_err(|e| ResolutionError::Str(e.to_string()))?;
        for (index, row) in rows.iter().enumerate() {
            if row.rhs == 0.0 {
                continue;
            }
            write!(out, "    RHS  R{index}  ").map_err(|e| ResolutionError::Str(e.to_string()))?;
            write_number(&mut out, row.rhs)?;
            out.push('\n');
        }
    }
    let mut bounds = String::new();
    for (index, definition) in defs.iter().enumerate() {
        let lower = definition.get_min();
        let upper = definition.get_max();
        if lower == upper {
            writeln!(bounds, " FX BND  X{index}  {lower:.17e}")
                .map_err(|e| ResolutionError::Str(e.to_string()))?;
        } else {
            if lower != 0.0 {
                writeln!(bounds, " LO BND  X{index}  {lower:.17e}")
                    .map_err(|e| ResolutionError::Str(e.to_string()))?;
            }
            if upper.is_finite() {
                writeln!(bounds, " UP BND  X{index}  {upper:.17e}")
                    .map_err(|e| ResolutionError::Str(e.to_string()))?;
            }
        }
    }
    if !bounds.is_empty() {
        out.push_str("BOUNDS\n");
        out.push_str(&bounds);
    }
    out.push_str("ENDATA\n");
    std::fs::write(path, out).map_err(|e| ResolutionError::Str(format!("SoPlex MPS write: {e}")))
}

fn status_error(status: soplex_rs::Status) -> ResolutionError {
    use soplex_rs::Status;
    match status {
        Status::Unbounded => ResolutionError::Unbounded,
        Status::Infeasible => ResolutionError::Infeasible,
        Status::AbortTime => ResolutionError::Str("SoPlex time limit reached".into()),
        Status::AbortIter => ResolutionError::Str("SoPlex iteration limit reached".into()),
        Status::AbortCycling => ResolutionError::Str("SoPlex aborted on cycling".into()),
        Status::InfOrUnbd => ResolutionError::Str("SoPlex reports infeasible or unbounded".into()),
        Status::OptimalUnscaledViolations => ResolutionError::Str(
            "SoPlex optimal solution has unscaled constraint violations".into(),
        ),
        other => ResolutionError::Str(format!("SoPlex status: {other:?}")),
    }
}

pub(crate) fn solve(
    defs: &[VariableDefinition],
    objective: &[(usize, f64)],
    rows: &[ParsedRow],
) -> Result<Vec<f64>, ResolutionError> {
    for row in rows {
        if row.terms.is_empty()
            && ((row.is_equality && row.rhs != 0.0) || (!row.is_equality && row.rhs < 0.0))
        {
            return Err(ResolutionError::Infeasible);
        }
    }
    let path = std::env::temp_dir().join(format!(
        "metatorio-soplex-{}-{}-{}.mps",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |value| value.as_nanos())
    ));
    let _guard = TempMps(path.clone());
    write_mps(&path, defs, objective, rows)?;

    let solved = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        use soplex_rs::{
            BoolParam, CheckMode, Model, ReadMode, RealParam, SolveMode, SyncMode, Verbosity,
        };
        let mut model = Model::new();
        model.set_verbosity(Verbosity::Error);
        model.set_bool_param(BoolParam::ForceBasic, true);
        model.set_read_mode(ReadMode::Rational);
        model.set_solve_mode(SolveMode::Rational);
        model.set_check_mode(CheckMode::Rational);
        model.set_sync_mode(SyncMode::Auto);
        model.set_real_param(RealParam::FeasTol, 0.0);
        model.set_real_param(RealParam::OptTol, 0.0);
        model.set_real_param(RealParam::TimeLimit, time_limit_seconds());
        let path = path
            .to_str()
            .ok_or_else(|| "non-UTF-8 MPS path".to_string())?;
        model.read_file(path);
        Ok::<_, String>(model.optimize())
    }))
    .map_err(|_| ResolutionError::Str("SoPlex panicked while reading or solving".into()))?
    .map_err(ResolutionError::Str)?;

    let status = solved.status();
    if status != soplex_rs::Status::Optimal {
        return Err(status_error(status));
    }
    let values = solved.primal_solution();
    if values.len() != defs.len() {
        return Err(ResolutionError::Str(format!(
            "SoPlex returned {} values for {} columns",
            values.len(),
            defs.len()
        )));
    }
    eprintln!(
        "[LP-SOPLEX] columns={} rows={} iters={}",
        defs.len(),
        rows.len(),
        solved.num_iterations()
    );
    Ok(values)
}
