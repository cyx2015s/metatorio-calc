//! Opt-in, bit-preserving LP capture and independent replay diagnostics.
//! Snapshots describe the f64 model, not the game's exact rational coefficients.

use std::{
    fs, io,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use good_lp::VariableDefinition;
use highs::{RowProblem, Sense};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub label: String,
    pub lower_bits: u64,
    pub upper_bits: u64,
    pub integer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    pub label: String,
    pub equality: bool,
    pub rhs_bits: u64,
    pub terms: Vec<(usize, u64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub format_version: u32,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    pub objective: Vec<(usize, u64)>,
    pub objective_offset_bits: u64,
    pub production_rhs_scale_bits: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowDiagnostic {
    pub index: usize,
    pub label: String,
    pub lhs: f64,
    pub rhs: f64,
    pub magnitude: f64,
    pub absolute_violation: f64,
    pub relative_violation: f64,
    pub naive_lhs: f64,
    pub largest_contributions: Vec<(usize, String, f64, f64, f64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Validation {
    pub finite: bool,
    pub maximum_relative_violation: f64,
    pub maximum_bound_violation: f64,
    pub objective: f64,
    pub worst_rows: Vec<RowDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DualValidation {
    pub finite: bool,
    pub maximum_relative_stationarity_error: f64,
    pub maximum_dual_sign_violation: f64,
    pub maximum_absolute_complementarity_error: f64,
    pub dual_objective: f64,
    pub relative_objective_gap: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairChange {
    pub index: usize,
    pub label: String,
    pub before_bits: u64,
    pub after_bits: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairDiagnostic {
    pub changes: Vec<RepairChange>,
    pub validation: Validation,
    pub dual_validation: DualValidation,
}

#[derive(Default, Clone)]
struct Compensated {
    sum: f64,
    correction: f64,
}

impl Compensated {
    fn add_product(&mut self, a: f64, b: f64) {
        let product = a * b;
        self.correction += a.mul_add(b, -product);
        let next = self.sum + product;
        self.correction += if self.sum.abs() >= product.abs() {
            (self.sum - next) + product
        } else {
            (product - next) + self.sum
        };
        self.sum = next;
    }
    fn value(&self) -> f64 {
        self.sum + self.correction
    }
}

/// Neumaier summation with a compensated product using f64's fused operation.
/// This reduces summation error; it is not an exact-arithmetic certificate.
fn dot(terms: impl Iterator<Item = (f64, f64)>) -> f64 {
    let mut accumulator = Compensated::default();
    for (a, b) in terms {
        accumulator.add_product(a, b);
    }
    accumulator.value()
}

impl Snapshot {
    pub fn exact_model(&self) -> crate::lp_rational::Model {
        use crate::lp_rational::{Column, Model, Row};
        let mut costs = vec![0.0; self.columns.len()];
        for (i, bits) in &self.objective {
            costs[*i] += f64::from_bits(*bits);
        }
        Model {
            columns: self
                .columns
                .iter()
                .zip(costs)
                .map(|(c, cost)| Column {
                    lower: f64::from_bits(c.lower_bits),
                    upper: f64::from_bits(c.upper_bits),
                    cost,
                })
                .collect(),
            rows: self
                .rows
                .iter()
                .map(|r| Row {
                    equality: r.equality,
                    rhs: f64::from_bits(r.rhs_bits),
                    terms: r
                        .terms
                        .iter()
                        .map(|(i, a)| (*i, f64::from_bits(*a)))
                        .collect(),
                })
                .collect(),
            offset: f64::from_bits(self.objective_offset_bits),
        }
    }
    /// Freeze zero columns outside the current support. Include every column of
    /// a violated row so missing producers can enter. No magnitude cutoff is
    /// used. A restricted optimum is not evidence of global optimality.
    pub fn restrict_support(&self, values: &[f64]) -> Result<(Self, Vec<usize>), String> {
        self.check()?;
        if values.len() != self.columns.len() || values.iter().any(|v| !v.is_finite()) {
            return Err("support requires a finite vector of the original dimension".into());
        }
        let mut keep: Vec<_> = self
            .columns
            .iter()
            .zip(values)
            .map(|(column, value)| {
                *value != 0.0
                    || f64::from_bits(column.lower_bits) > 0.0
                    || f64::from_bits(column.upper_bits) < 0.0
            })
            .collect();
        for row in &self.rows {
            let lhs = dot(row
                .terms
                .iter()
                .map(|(i, a)| (f64::from_bits(*a), values[*i])));
            let rhs = f64::from_bits(row.rhs_bits);
            let raw = if row.equality {
                (lhs - rhs).abs()
            } else {
                (lhs - rhs).max(0.0)
            };
            let magnitude: f64 = row
                .terms
                .iter()
                .map(|(i, a)| (f64::from_bits(*a) * values[*i]).abs())
                .sum();
            if raw > 1e-6 * magnitude.max(rhs.abs()) + 1e-8 {
                for (column, _) in &row.terms {
                    keep[*column] = true;
                }
            }
        }
        let selected: Vec<_> = keep
            .iter()
            .enumerate()
            .filter_map(|(i, keep)| keep.then_some(i))
            .collect();
        let mut remap = vec![None; keep.len()];
        for (new, old) in selected.iter().enumerate() {
            remap[*old] = Some(new);
        }
        let terms = |entries: &[(usize, u64)]| {
            entries
                .iter()
                .filter_map(|(i, a)| remap[*i].map(|new| (new, *a)))
                .collect()
        };
        Ok((
            Self {
                columns: selected.iter().map(|i| self.columns[*i].clone()).collect(),
                rows: self
                    .rows
                    .iter()
                    .map(|row| Row {
                        label: row.label.clone(),
                        equality: row.equality,
                        rhs_bits: row.rhs_bits,
                        terms: terms(&row.terms),
                    })
                    .collect(),
                objective: terms(&self.objective),
                ..self.clone()
            },
            selected,
        ))
    }
    /// Experimental coordinate repair, without a magnitude cutoff: a zero-cost
    /// column may be lowered to its bound only if it participates exclusively
    /// in inequalities, fixes a violated row, and every affected row then
    /// satisfies its original inequality. Required target/equality flows stay.
    pub fn repair_zero_cost_columns(&self, values: &[f64]) -> (Vec<f64>, Vec<RepairChange>) {
        if values.len() != self.columns.len() || values.iter().any(|v| !v.is_finite()) {
            return (values.to_vec(), Vec::new());
        }
        let mut costs = vec![0.0; self.columns.len()];
        for (index, a) in &self.objective {
            costs[*index] += f64::from_bits(*a);
        }
        let mut incidence = vec![Vec::new(); self.columns.len()];
        for (index, row) in self.rows.iter().enumerate() {
            for (column, a) in &row.terms {
                if f64::from_bits(*a) != 0.0 {
                    incidence[*column].push(index);
                }
            }
        }
        let mut repaired = values.to_vec();
        let mut changes = Vec::new();
        for _ in 0..4 {
            let mut progress = false;
            for (column, affected) in incidence.iter().enumerate() {
                let lower = f64::from_bits(self.columns[column].lower_bits);
                if costs[column] != 0.0
                    || !lower.is_finite()
                    || repaired[column] <= lower
                    || affected.is_empty()
                    || affected.iter().any(|i| self.rows[*i].equality)
                {
                    continue;
                }
                let violated = affected.iter().any(|i| {
                    let row = &self.rows[*i];
                    dot(row
                        .terms
                        .iter()
                        .map(|(j, a)| (f64::from_bits(*a), repaired[*j])))
                        > f64::from_bits(row.rhs_bits)
                });
                if !violated {
                    continue;
                }
                let original = repaired[column];
                repaired[column] = lower;
                let feasible = affected.iter().all(|i| {
                    let row = &self.rows[*i];
                    let activity = dot(row
                        .terms
                        .iter()
                        .map(|(j, a)| (f64::from_bits(*a), repaired[*j])));
                    activity.is_finite() && activity <= f64::from_bits(row.rhs_bits)
                });
                if feasible {
                    changes.push(RepairChange {
                        index: column,
                        label: self.columns[column].label.clone(),
                        before_bits: original.to_bits(),
                        after_bits: lower.to_bits(),
                    });
                    progress = true;
                } else {
                    repaired[column] = original;
                }
            }
            if !progress {
                break;
            }
        }
        (repaired, changes)
    }

    pub fn check(&self) -> Result<(), String> {
        if self.format_version != 1 {
            return Err("unsupported snapshot version".into());
        }
        for column in &self.columns {
            let lower = f64::from_bits(column.lower_bits);
            let upper = f64::from_bits(column.upper_bits);
            if lower.is_nan() || upper.is_nan() || lower > upper || column.integer {
                return Err("replay requires valid continuous variable bounds".into());
            }
        }
        for (index, bits) in self
            .objective
            .iter()
            .chain(self.rows.iter().flat_map(|r| &r.terms))
        {
            if *index >= self.columns.len() || !f64::from_bits(*bits).is_finite() {
                return Err("invalid matrix/objective entry".into());
            }
        }
        if !f64::from_bits(self.objective_offset_bits).is_finite()
            || self
                .rows
                .iter()
                .any(|r| !f64::from_bits(r.rhs_bits).is_finite())
        {
            return Err("non-finite objective offset or RHS".into());
        }
        Ok(())
    }

    pub fn validate(&self, values: &[f64]) -> Validation {
        let mut result = Validation {
            finite: values.len() == self.columns.len() && values.iter().all(|v| v.is_finite()),
            maximum_relative_violation: 0.0,
            maximum_bound_violation: 0.0,
            objective: 0.0,
            worst_rows: Vec::new(),
        };
        if !result.finite {
            return result;
        }
        for (column, &value) in self.columns.iter().zip(values) {
            result.maximum_bound_violation = result.maximum_bound_violation.max(
                (f64::from_bits(column.lower_bits) - value)
                    .max(value - f64::from_bits(column.upper_bits))
                    .max(0.0),
            );
        }
        result.objective = dot(self
            .objective
            .iter()
            .map(|(i, a)| (f64::from_bits(*a), values[*i])))
            + f64::from_bits(self.objective_offset_bits);
        for (index, row) in self.rows.iter().enumerate() {
            let rhs = f64::from_bits(row.rhs_bits);
            let lhs = dot(row
                .terms
                .iter()
                .map(|(i, a)| (f64::from_bits(*a), values[*i])));
            let magnitude: f64 = row
                .terms
                .iter()
                .map(|(i, a)| (f64::from_bits(*a) * values[*i]).abs())
                .sum();
            if !lhs.is_finite() || !magnitude.is_finite() {
                result.finite = false;
                continue;
            }
            let raw = if row.equality {
                (lhs - rhs).abs()
            } else {
                (lhs - rhs).max(0.0)
            };
            let scale = magnitude.max(rhs.abs());
            // Identical acceptance budget to production; keep raw diagnostics too.
            let relative = if raw <= 1e-6 * scale + 1e-8 {
                0.0
            } else {
                raw / scale.max(f64::MIN_POSITIVE)
            };
            result.maximum_relative_violation = result.maximum_relative_violation.max(relative);
            if raw == 0.0 {
                continue;
            }
            let mut contributions: Vec<_> = row
                .terms
                .iter()
                .map(|(i, a)| {
                    let coefficient = f64::from_bits(*a);
                    (
                        *i,
                        self.columns[*i].label.clone(),
                        coefficient,
                        values[*i],
                        coefficient * values[*i],
                    )
                })
                .collect();
            contributions.sort_by(|a, b| b.4.abs().total_cmp(&a.4.abs()));
            contributions.truncate(8);
            result.worst_rows.push(RowDiagnostic {
                index,
                label: row.label.clone(),
                lhs,
                rhs,
                magnitude,
                absolute_violation: raw,
                relative_violation: relative,
                naive_lhs: row
                    .terms
                    .iter()
                    .map(|(i, a)| f64::from_bits(*a) * values[*i])
                    .sum(),
                largest_contributions: contributions,
            });
        }
        result.worst_rows.sort_by(|a, b| {
            b.relative_violation
                .total_cmp(&a.relative_violation)
                .then_with(|| b.absolute_violation.total_cmp(&a.absolute_violation))
        });
        result.worst_rows.truncate(12);
        result.finite &= result.objective.is_finite();
        result
    }

    /// Checks minimization duals in the original model's units. This is a
    /// numerical diagnostic, not a rigorous bound or an exact certificate.
    pub fn validate_dual(
        &self,
        values: &[f64],
        row_duals: &[f64],
        reduced_costs: &[f64],
    ) -> DualValidation {
        let mut report = DualValidation {
            finite: values.len() == self.columns.len()
                && reduced_costs.len() == self.columns.len()
                && row_duals.len() == self.rows.len()
                && values
                    .iter()
                    .chain(row_duals)
                    .chain(reduced_costs)
                    .all(|v| v.is_finite()),
            maximum_relative_stationarity_error: 0.0,
            maximum_dual_sign_violation: 0.0,
            maximum_absolute_complementarity_error: 0.0,
            dual_objective: 0.0,
            relative_objective_gap: 0.0,
        };
        if !report.finite {
            return report;
        }
        let mut aty = vec![Compensated::default(); self.columns.len()];
        let mut aty_magnitude = vec![0.0; self.columns.len()];
        let mut dual_objective = Compensated::default();
        for (row, &y) in self.rows.iter().zip(row_duals) {
            if !row.equality {
                report.maximum_dual_sign_violation =
                    report.maximum_dual_sign_violation.max(y.max(0.0));
            }
            let rhs = f64::from_bits(row.rhs_bits);
            dual_objective.add_product(rhs, y);
            let activity = dot(row
                .terms
                .iter()
                .map(|(i, a)| (f64::from_bits(*a), values[*i])));
            report.maximum_absolute_complementarity_error = report
                .maximum_absolute_complementarity_error
                .max((y * (activity - rhs)).abs());
            for (index, a) in &row.terms {
                let a = f64::from_bits(*a);
                aty[*index].add_product(a, y);
                aty_magnitude[*index] += (a * y).abs();
            }
        }
        let mut costs = vec![0.0; self.columns.len()];
        for (index, a) in &self.objective {
            costs[*index] += f64::from_bits(*a);
        }
        for (index, column) in self.columns.iter().enumerate() {
            let z = reduced_costs[index];
            let lower = f64::from_bits(column.lower_bits);
            let upper = f64::from_bits(column.upper_bits);
            let error = (costs[index] - aty[index].value() - z).abs();
            let scale = costs[index]
                .abs()
                .max(aty_magnitude[index] + z.abs())
                .max(1.0);
            report.maximum_relative_stationarity_error = report
                .maximum_relative_stationarity_error
                .max(error / scale);
            let bound = if z >= 0.0 { lower } else { upper };
            if z != 0.0 {
                if bound.is_finite() {
                    dual_objective.add_product(z, bound);
                    report.maximum_absolute_complementarity_error = report
                        .maximum_absolute_complementarity_error
                        .max((z * (values[index] - bound)).abs());
                } else {
                    report.maximum_dual_sign_violation =
                        report.maximum_dual_sign_violation.max(z.abs());
                }
            }
            report.finite &= error.is_finite() && scale.is_finite();
        }
        report.dual_objective = dual_objective.value() + f64::from_bits(self.objective_offset_bits);
        let primal = dot(self
            .objective
            .iter()
            .map(|(i, a)| (f64::from_bits(*a), values[*i])))
            + f64::from_bits(self.objective_offset_bits);
        report.relative_objective_gap = (primal - report.dual_objective).abs()
            / primal.abs().max(report.dual_objective.abs()).max(1.0);
        report.finite &= report.dual_objective.is_finite()
            && report.relative_objective_gap.is_finite()
            && report.maximum_absolute_complementarity_error.is_finite();
        report
    }
}

pub(crate) fn column(definition: &VariableDefinition, index: usize) -> Column {
    Column {
        label: if definition.get_name().is_empty() {
            format!("x{index}")
        } else {
            definition.get_name().into()
        },
        lower_bits: definition.get_min().to_bits(),
        upper_bits: definition.get_max().to_bits(),
        integer: definition.is_integer(),
    }
}

pub(crate) fn capture(snapshot: &Snapshot) -> Option<std::path::PathBuf> {
    let directory = std::env::var_os("METATORIO_LP_CAPTURE_DIR")?;
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let filename = format!(
        "lp-{}-{run}-{}.json",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    );
    let save = || -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(&directory)?;
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(Path::new(&directory).join(&filename))?;
        serde_json::to_writer(io::BufWriter::new(file), snapshot)?;
        Ok(())
    };
    match save() {
        Ok(()) => {
            eprintln!(
                "[LP-CAPTURE] {filename}: {} columns, {} rows",
                snapshot.columns.len(),
                snapshot.rows.len()
            );
            Some(Path::new(&directory).join(&filename))
        }
        Err(error) => {
            eprintln!("[LP-CAPTURE-ERROR] {error}");
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayOptions {
    pub name: String,
    pub rhs_scale: f64,
    pub solver: String,
    pub scaling: Option<i32>,
    pub presolve: bool,
    pub tolerance: Option<f64>,
    pub small_matrix_value: Option<f64>,
    pub time_limit: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReplayResult {
    pub highs_version: String,
    pub options: ReplayOptions,
    pub status: String,
    pub primal_status: String,
    pub elapsed_seconds: f64,
    pub simplex_iterations: i64,
    pub ipm_iterations: i64,
    pub validation: Validation,
    pub dual_validation: DualValidation,
    pub repair: RepairDiagnostic,
    pub values_bits: Vec<u64>,
    pub row_duals_bits: Vec<u64>,
    pub column_duals_bits: Vec<u64>,
    pub basis_columns: Vec<i32>,
    pub basis_rows: Vec<i32>,
    pub basis_status: i32,
    pub load_status: i32,
    pub loaded_nonzeros: i64,
}

pub fn replay(snapshot: &Snapshot, options: ReplayOptions) -> Result<ReplayResult, String> {
    snapshot.check()?;
    if !options.rhs_scale.is_finite() || options.rhs_scale <= 0.0 {
        return Err("invalid scale".into());
    }
    let scale = options.rhs_scale;
    let mut costs = vec![0.0; snapshot.columns.len()];
    for (index, bits) in &snapshot.objective {
        costs[*index] += f64::from_bits(*bits)
    }
    let lower: Vec<_> = snapshot
        .columns
        .iter()
        .map(|c| f64::from_bits(c.lower_bits) * scale)
        .collect();
    let upper: Vec<_> = snapshot
        .columns
        .iter()
        .map(|c| f64::from_bits(c.upper_bits) * scale)
        .collect();
    for ((column, lower), upper) in snapshot.columns.iter().zip(&lower).zip(&upper) {
        if (f64::from_bits(column.lower_bits).is_finite() && !lower.is_finite())
            || (f64::from_bits(column.upper_bits).is_finite() && !upper.is_finite())
        {
            return Err("scaled bound overflow".into());
        }
    }
    let mut row_lower = Vec::new();
    let mut row_upper = Vec::new();
    let mut starts = vec![0 as highs_sys::HighsInt];
    let mut indices = Vec::new();
    let mut coefficients = Vec::new();
    for row in &snapshot.rows {
        let rhs = f64::from_bits(row.rhs_bits) * scale;
        if !rhs.is_finite() {
            return Err("scaled RHS overflow".into());
        }
        row_lower.push(if row.equality { rhs } else { f64::NEG_INFINITY });
        row_upper.push(rhs);
        for (column, bits) in &row.terms {
            indices
                .push(highs_sys::HighsInt::try_from(*column).map_err(|_| "column index overflow")?);
            coefficients.push(f64::from_bits(*bits));
        }
        starts.push(
            highs_sys::HighsInt::try_from(indices.len()).map_err(|_| "matrix size overflow")?,
        );
    }
    // Configure an empty model first: small_matrix_value filters coefficients
    // during passLp, so setting it after loading silently loses small entries.
    let mut model = RowProblem::default()
        .try_optimise(Sense::Minimise)
        .map_err(|e| format!("load: {e:?}"))?;
    macro_rules! option {
        ($key:expr,$value:expr) => {
            model
                .try_set_option($key, $value)
                .map_err(|e| format!("option {}: {e:?}", $key))?
        };
    }
    option!("output_flag", false);
    option!("solver", options.solver.as_str());
    option!("presolve", if options.presolve { "on" } else { "off" });
    option!("run_crossover", "on");
    option!("time_limit", options.time_limit);
    option!("threads", 1_i32);
    if let Some(value) = options.scaling {
        option!("simplex_scale_strategy", value);
    }
    if let Some(value) = options.tolerance {
        option!("primal_feasibility_tolerance", value);
        option!("dual_feasibility_tolerance", value);
        option!("ipm_optimality_tolerance", value);
    }
    if let Some(value) = options.small_matrix_value {
        option!("small_matrix_value", value);
    }
    let num_columns = highs_sys::HighsInt::try_from(snapshot.columns.len())
        .map_err(|_| "column count overflow")?;
    let num_rows =
        highs_sys::HighsInt::try_from(snapshot.rows.len()).map_err(|_| "row count overflow")?;
    let num_nonzero =
        highs_sys::HighsInt::try_from(indices.len()).map_err(|_| "matrix size overflow")?;
    // Checked CSR indices and starts describe all rows. All arrays live until
    // this synchronous call returns; HiGHS copies their contents into the model.
    let load_status = unsafe {
        highs_sys::Highs_passLp(
            model.as_ptr().cast_mut(),
            num_columns,
            num_rows,
            num_nonzero,
            highs_sys::MATRIX_FORMAT_ROW_WISE,
            highs_sys::OBJECTIVE_SENSE_MINIMIZE,
            0.0,
            costs.as_ptr(),
            lower.as_ptr(),
            upper.as_ptr(),
            row_lower.as_ptr(),
            row_upper.as_ptr(),
            starts.as_ptr(),
            indices.as_ptr(),
            coefficients.as_ptr(),
        )
    };
    if load_status == highs_sys::STATUS_ERROR {
        return Err("load: HiGHS rejected the original matrix".into());
    }
    // Read-only query on the same live model, before any presolve reduction.
    let loaded_nonzeros = unsafe { highs_sys::Highs_getNumNz(model.as_ptr()) } as i64;
    let start = Instant::now();
    let solved = model.try_solve().map_err(|e| format!("solve: {e:?}"))?;
    let elapsed_seconds = start.elapsed().as_secs_f64();
    let solution = solved.get_solution();
    let values: Vec<_> = solution.columns().iter().map(|v| v / scale).collect();
    let (repaired, changes) = snapshot.repair_zero_cost_columns(&values);
    let mut basis_columns = vec![0; snapshot.columns.len()];
    let mut basis_rows = vec![0; snapshot.rows.len()];
    // Owned arrays exactly match the live model dimensions; HiGHS writes them synchronously.
    let basis_status = unsafe {
        highs_sys::Highs_getBasis(
            solved.as_ptr(),
            basis_columns.as_mut_ptr(),
            basis_rows.as_mut_ptr(),
        )
    };
    // HiGHS exposes a process-lifetime, NUL-terminated version string.
    let highs_version = unsafe { std::ffi::CStr::from_ptr(highs_sys::Highs_version()) }
        .to_string_lossy()
        .into_owned();
    Ok(ReplayResult {
        highs_version,
        options,
        status: format!("{:?}", solved.status()),
        primal_status: format!("{:?}", solved.primal_solution_status()),
        elapsed_seconds,
        simplex_iterations: solved.simplex_iteration_count(),
        ipm_iterations: solved.ipm_iteration_count(),
        validation: snapshot.validate(&values),
        dual_validation: snapshot.validate_dual(
            &values,
            solution.dual_rows(),
            solution.dual_columns(),
        ),
        repair: RepairDiagnostic {
            changes,
            validation: snapshot.validate(&repaired),
            dual_validation: snapshot.validate_dual(
                &repaired,
                solution.dual_rows(),
                solution.dual_columns(),
            ),
        },
        values_bits: values.iter().map(|v| v.to_bits()).collect(),
        row_duals_bits: solution.dual_rows().iter().map(|v| v.to_bits()).collect(),
        column_duals_bits: solution
            .dual_columns()
            .iter()
            .map(|v| v.to_bits())
            .collect(),
        basis_columns,
        basis_rows,
        basis_status,
        load_status,
        loaded_nonzeros,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model() -> Snapshot {
        Snapshot {
            format_version: 1,
            columns: vec![Column {
                label: "x".into(),
                lower_bits: 0.0f64.to_bits(),
                upper_bits: f64::INFINITY.to_bits(),
                integer: false,
            }],
            rows: vec![Row {
                label: "target".into(),
                equality: true,
                rhs_bits: 1.0f64.to_bits(),
                terms: vec![(0, 1.0f64.to_bits())],
            }],
            objective: vec![(0, 1.0f64.to_bits())],
            objective_offset_bits: 0.0f64.to_bits(),
            production_rhs_scale_bits: 1e4f64.to_bits(),
        }
    }
    #[test]
    fn bit_roundtrip_and_reject_missing_supply() {
        let original = model();
        let copy: Snapshot =
            serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap();
        assert_eq!(copy.columns[0].upper_bits, f64::INFINITY.to_bits());
        assert_eq!(copy.validate(&[1.0]).maximum_relative_violation, 0.0);
        assert_eq!(copy.validate(&[0.0]).maximum_relative_violation, 1.0);
        assert!(!copy.validate(&[f64::NAN]).finite);
        assert_eq!(copy.validate(&[-1.0]).maximum_bound_violation, 1.0);
        let dual = copy.validate_dual(&[1.0], &[1.0], &[0.0]);
        assert!(dual.finite);
        assert_eq!(dual.relative_objective_gap, 0.0);
        assert_eq!(dual.maximum_relative_stationarity_error, 0.0);
        assert_eq!(
            copy.validate_dual(&[1.0], &[0.0], &[0.0])
                .maximum_relative_stationarity_error,
            1.0
        );
    }
    #[test]
    fn compensated_cancellation_and_scaled_finite_bound() {
        assert_eq!(
            dot([(1e16, 1.0), (1.0, 1.0), (-1e16, 1.0)].into_iter()),
            1.0
        );
        let mut snapshot = model();
        snapshot.columns[0].upper_bits = 1.0f64.to_bits();
        let result = replay(
            &snapshot,
            ReplayOptions {
                name: "test".into(),
                rhs_scale: 1e4,
                solver: "simplex".into(),
                scaling: None,
                presolve: true,
                tolerance: None,
                small_matrix_value: None,
                time_limit: 10.0,
            },
        )
        .unwrap();
        assert_eq!(result.status, "Optimal");
        assert_eq!(f64::from_bits(result.values_bits[0]), 1.0);
        assert!(result.validation.finite);
        assert_eq!(result.validation.maximum_relative_violation, 0.0);
    }

    #[test]
    fn repair_removes_orphan_bridge_but_keeps_required_tiny_flow() {
        let mut snapshot = model();
        snapshot.objective.clear();
        snapshot.rows = vec![
            Row {
                label: "supply".into(),
                equality: false,
                rhs_bits: 0.0f64.to_bits(),
                terms: vec![(0, 1.0f64.to_bits())],
            },
            Row {
                label: "fuel".into(),
                equality: false,
                rhs_bits: 0.0f64.to_bits(),
                terms: vec![(0, (-1.0f64).to_bits())],
            },
        ];
        let (values, changes) = snapshot.repair_zero_cost_columns(&[1.22e-8]);
        assert_eq!(values, vec![0.0]);
        assert_eq!(changes.len(), 1);
        assert_eq!(snapshot.validate(&values).maximum_relative_violation, 0.0);
        snapshot.rows.push(Row {
            label: "tiny target".into(),
            equality: true,
            rhs_bits: 1.22e-8f64.to_bits(),
            terms: vec![(0, 1.0f64.to_bits())],
        });
        let (values, changes) = snapshot.repair_zero_cost_columns(&[1.22e-8]);
        assert!(changes.is_empty());
        assert_eq!(values, vec![1.22e-8]);
    }

    #[test]
    fn support_keeps_missing_producer_and_required_bound() {
        let mut snapshot = model();
        snapshot.columns.extend([
            Column {
                label: "producer".into(),
                ..snapshot.columns[0].clone()
            },
            Column {
                label: "unused".into(),
                ..snapshot.columns[0].clone()
            },
            Column {
                label: "required".into(),
                lower_bits: 0.5f64.to_bits(),
                ..snapshot.columns[0].clone()
            },
        ]);
        snapshot.rows.push(Row {
            label: "supply".into(),
            equality: false,
            rhs_bits: 0.0f64.to_bits(),
            terms: vec![(0, 1.0f64.to_bits()), (1, (-1.0f64).to_bits())],
        });
        let (restricted, selected) = snapshot.restrict_support(&[1.0, 0.0, 0.0, 0.5]).unwrap();
        assert_eq!(selected, vec![0, 1, 3]);
        assert_eq!(restricted.columns.len(), 3);
        assert_eq!(restricted.rows[1].terms[1].0, 1);
    }

    #[test]
    fn small_matrix_option_is_applied_before_loading() {
        let mut snapshot = model();
        snapshot.rows[0].terms[0].1 = 1e-10f64.to_bits();
        snapshot.rows[0].rhs_bits = 1e-10f64.to_bits();
        let options = ReplayOptions {
            name: "small".into(),
            rhs_scale: 1e4,
            solver: "simplex".into(),
            scaling: None,
            presolve: true,
            tolerance: Some(1e-10),
            small_matrix_value: Some(1e-12),
            time_limit: 10.0,
        };
        let result = replay(&snapshot, options).unwrap();
        assert_eq!(result.status, "Optimal");
        assert!((f64::from_bits(result.values_bits[0]) - 1.0).abs() < 1e-12);
    }
}
