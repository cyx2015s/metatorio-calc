//! 基于 HiGHS 原生增量加列的列生成主问题（RMP）封装。
//!
//! 直接操作 `highs::Model`，支持：
//! - 保留单纯形基热启动的 `add_col` / `add_row`；
//! - 从 `SolvedModel::get_solution().dual_rows()` 提取每个物品/目标的有效影子价格；
//! - 当初始种子集不可行时，自动构造 Phase I（人工松弛变量 + Farkas 对偶价格）辅助问题。

use highs::{ColProblem, HighsModelStatus, Model, Row, Sense};

use crate::concept::{AIndexMap, AIndexSet, Flow, ItemIdent};
use crate::lp_model::ResolutionError;
use crate::solver::{SolverData, TargetSpec};

/// 主问题一次 Phase II 求解后的状态。
#[derive(Debug)]
pub enum MasterSolveState<I, F>
where
    I: ItemIdent,
    F: ItemIdent,
{
    Optimal {
        cost: f64,
        primal_flows: AIndexMap<F, f64>,
        effective_duals: AIndexMap<I, f64>,
        num_cols: usize,
        num_rows: usize,
    },
    Infeasible,
    Unbounded,
}

/// Phase I 辅助问题求解结果（永远可行且下界为 0）。
#[derive(Debug)]
pub struct Phase1State<I>
where
    I: ItemIdent,
{
    /// 人工变量之和；`<= 1e-9` 表示当前列集已可行。
    pub artificial_sum: f64,
    /// 各物品/流对应的 Phase I（Farkas）有效影子价格。
    pub effective_duals: AIndexMap<I, f64>,
}

struct StoredFlow<I, F>
where
    I: ItemIdent,
{
    id: F,
    coefficients: Flow<I>,
    cost: f64,
    col_index: usize,
}

/// HiGHS 增量列生成主问题（Restricted Master Problem）。
pub struct HighsColumnGenMaster<I, F>
where
    I: ItemIdent,
    F: ItemIdent,
{
    model: Option<Model>,
    strict_sink: bool,
    unconstrained: AIndexSet<I>,
    item_in_targets: AIndexSet<I>,
    targets: Vec<TargetSpec<I>>,
    sources: Flow<I>,
    sinks: Flow<I>,
    /// 物品 -> (HiGHS Row 句柄, 行在 dual_rows 中的下标)
    item_to_row: AIndexMap<I, (Row, usize)>,
    /// 每个目标表达式 -> (HiGHS Row 句柄, 行在 dual_rows 中的下标)
    target_rows: Vec<(Row, usize)>,
    flows: Vec<StoredFlow<I, F>>,
    total_cols: usize,
    total_rows: usize,
}

impl<I, F> HighsColumnGenMaster<I, F>
where
    I: ItemIdent,
    F: ItemIdent,
{
    /// 从初始种子 `SolverData` 构建 HiGHS 受限主问题。
    ///
    /// 为确保 Phase I 与延迟列生成不丢失行，本构造函数**不做** `no_providers` 行删除，
    /// 所有非目标受约束物品均保留平衡行（`strict_sink` 决定 `== 0` 还是 `>= 0`）。
    pub fn from_seed(data: &SolverData<I, F>) -> Result<Self, ResolutionError> {
        let mut item_in_targets: AIndexSet<I> = AIndexSet::default();
        for target in &data.target {
            for (item, &coef) in &target.coefficients {
                if coef != 0.0 {
                    item_in_targets.insert(item.clone());
                }
            }
        }

        let mut balance_items: AIndexSet<I> = AIndexSet::default();
        for spec in data.flows.values() {
            for (item, &coef) in &spec.coefficients {
                if coef != 0.0
                    && !data.unconstrained.contains(item)
                    && !item_in_targets.contains(item)
                {
                    balance_items.insert(item.clone());
                }
            }
        }
        for item in data.sources.keys().chain(data.sinks.keys()) {
            if !data.unconstrained.contains(item) && !item_in_targets.contains(item) {
                balance_items.insert(item.clone());
            }
        }

        let mut pb = ColProblem::new();
        let mut total_rows = 0usize;

        // 1) 添加非目标物品平衡行
        let mut item_to_row: AIndexMap<I, (Row, usize)> = AIndexMap::default();
        for item in &balance_items {
            let row = if data.strict_sink {
                pb.add_row(0.0..=0.0)
            } else {
                pb.add_row(0.0..)
            };
            item_to_row.insert(item.clone(), (row, total_rows));
            total_rows += 1;
        }

        // 2) 添加目标约束行
        let mut target_rows: Vec<(Row, usize)> = Vec::with_capacity(data.target.len());
        for target in &data.target {
            let row = pb.add_row(target.constant..=target.constant);
            target_rows.push((row, total_rows));
            total_rows += 1;
        }

        let collect_item_factors = |item: &I, sign: f64| -> Vec<(Row, f64)> {
            if data.unconstrained.contains(item) {
                return Vec::new();
            }
            if item_in_targets.contains(item) {
                let mut acc: AIndexMap<usize, (Row, f64)> = AIndexMap::default();
                for (t_idx, target) in data.target.iter().enumerate() {
                    if let Some(&t_coef) = target.coefficients.get(item)
                        && t_coef != 0.0
                    {
                        let (row, r_idx) = target_rows[t_idx];
                        let entry = acc.entry(r_idx).or_insert((row, 0.0));
                        entry.1 += sign * t_coef;
                    }
                }
                acc.into_values().filter(|(_, v)| *v != 0.0).collect()
            } else if let Some(&(row, _)) = item_to_row.get(item) {
                vec![(row, sign)]
            } else {
                Vec::new()
            }
        };

        let mut total_cols = 0usize;

        // 3) 外部源列
        for (item, &cost) in &data.sources {
            let factors = collect_item_factors(item, 1.0);
            pb.add_column(cost, 0.0.., factors);
            total_cols += 1;
        }

        // 4) 外部汇列
        for (item, &cost) in &data.sinks {
            let factors = collect_item_factors(item, -1.0);
            pb.add_column(cost, 0.0.., factors);
            total_cols += 1;
        }

        // 5) 初始种子流列
        let mut flows: Vec<StoredFlow<I, F>> = Vec::with_capacity(data.flows.len());
        for (flow_id, spec) in &data.flows {
            let mut acc: AIndexMap<usize, (Row, f64)> = AIndexMap::default();
            for (item, &coef) in &spec.coefficients {
                if coef == 0.0 || data.unconstrained.contains(item) {
                    continue;
                }
                if item_in_targets.contains(item) {
                    for (t_idx, target) in data.target.iter().enumerate() {
                        if let Some(&t_coef) = target.coefficients.get(item)
                            && t_coef != 0.0
                        {
                            let (row, r_idx) = target_rows[t_idx];
                            let entry = acc.entry(r_idx).or_insert((row, 0.0));
                            entry.1 += t_coef * coef;
                        }
                    }
                } else if let Some(&(row, r_idx)) = item_to_row.get(item) {
                    let entry = acc.entry(r_idx).or_insert((row, 0.0));
                    entry.1 += coef;
                }
            }
            let factors: Vec<(Row, f64)> = acc.into_values().filter(|(_, v)| *v != 0.0).collect();
            pb.add_column(spec.cost, 0.0.., factors);
            flows.push(StoredFlow {
                id: flow_id.clone(),
                coefficients: spec.coefficients.clone(),
                cost: spec.cost,
                col_index: total_cols,
            });
            total_cols += 1;
        }

        let mut model = pb
            .try_optimise(Sense::Minimise)
            .map_err(|e| ResolutionError::Str(format!("HiGHS init failed: {e:?}")))?;
        model.make_quiet();

        Ok(Self {
            model: Some(model),
            strict_sink: data.strict_sink,
            unconstrained: data.unconstrained.clone(),
            item_in_targets,
            targets: data.target.clone(),
            sources: data.sources.clone(),
            sinks: data.sinks.clone(),
            item_to_row,
            target_rows,
            flows,
            total_cols,
            total_rows,
        })
    }

    pub fn num_cols(&self) -> usize {
        self.total_cols
    }

    pub fn num_rows(&self) -> usize {
        self.total_rows
    }

    /// 增量追加一条新的生产流列（若含新出现的物品则自动追加对应平衡行）。
    pub fn add_flow(&mut self, flow_id: F, coefficients: Flow<I>, cost: f64) {
        let model = self
            .model
            .as_mut()
            .expect("HighsColumnGenMaster model missing");

        let mut row_factors: AIndexMap<usize, (Row, f64)> = AIndexMap::default();

        for (item, &coef) in &coefficients {
            if coef == 0.0 || self.unconstrained.contains(item) {
                continue;
            }
            if self.item_in_targets.contains(item) {
                for (t_idx, target) in self.targets.iter().enumerate() {
                    if let Some(&t_coef) = target.coefficients.get(item)
                        && t_coef != 0.0
                    {
                        let (row, r_idx) = self.target_rows[t_idx];
                        let entry = row_factors.entry(r_idx).or_insert((row, 0.0));
                        entry.1 += t_coef * coef;
                    }
                }
            } else {
                let (row, r_idx) = if let Some(&existing) = self.item_to_row.get(item) {
                    existing
                } else {
                    let empty: [(highs::Col, f64); 0] = [];
                    let new_row = if self.strict_sink {
                        model.add_row(0.0..=0.0, empty)
                    } else {
                        model.add_row(0.0.., empty)
                    };
                    let idx = self.total_rows;
                    self.item_to_row.insert(item.clone(), (new_row, idx));
                    self.total_rows += 1;
                    (new_row, idx)
                };
                let entry = row_factors.entry(r_idx).or_insert((row, 0.0));
                entry.1 += coef;
            }
        }

        let factors: Vec<(Row, f64)> = row_factors
            .into_values()
            .filter(|(_, v)| *v != 0.0)
            .collect();

        model.add_col(cost, 0.0.., factors);
        let col_index = self.total_cols;
        self.total_cols += 1;
        self.flows.push(StoredFlow {
            id: flow_id,
            coefficients,
            cost,
            col_index,
        });
    }

    /// 用当前全部活跃列重建干净的 `ColProblem`（启用 HiGHS 完整 presolve），
    /// 避免超大规模多星球模型在增量 `add_col` 热启动时跳过 presolve 导致单纯形退化卡死。
    fn rebuild_model(&self) -> Result<Model, ResolutionError> {
        let mut pb = ColProblem::new();
        let mut item_rows: AIndexMap<I, Row> = AIndexMap::default();
        for item in self.item_to_row.keys() {
            let row = if self.strict_sink {
                pb.add_row(0.0..=0.0)
            } else {
                pb.add_row(0.0..)
            };
            item_rows.insert(item.clone(), row);
        }
        let mut target_rows: Vec<Row> = Vec::with_capacity(self.targets.len());
        for target in &self.targets {
            target_rows.push(pb.add_row(target.constant..=target.constant));
        }

        let collect_item_factors = |item: &I, sign: f64| -> Vec<(Row, f64)> {
            if self.unconstrained.contains(item) {
                return Vec::new();
            }
            if self.item_in_targets.contains(item) {
                let mut acc: AIndexMap<usize, (Row, f64)> = AIndexMap::default();
                for (t_idx, target) in self.targets.iter().enumerate() {
                    if let Some(&t_coef) = target.coefficients.get(item)
                        && t_coef != 0.0
                    {
                        let row = target_rows[t_idx];
                        let entry = acc.entry(t_idx).or_insert((row, 0.0));
                        entry.1 += sign * t_coef;
                    }
                }
                acc.into_values().filter(|(_, v)| *v != 0.0).collect()
            } else if let Some(&row) = item_rows.get(item) {
                vec![(row, sign)]
            } else {
                Vec::new()
            }
        };

        for (item, &cost) in &self.sources {
            pb.add_column(cost, 0.0.., collect_item_factors(item, 1.0));
        }
        for (item, &cost) in &self.sinks {
            pb.add_column(cost, 0.0.., collect_item_factors(item, -1.0));
        }
        for stored in &self.flows {
            let mut acc: AIndexMap<usize, (Row, f64)> = AIndexMap::default();
            for (item, &coef) in &stored.coefficients {
                if coef == 0.0 || self.unconstrained.contains(item) {
                    continue;
                }
                if self.item_in_targets.contains(item) {
                    for (t_idx, target) in self.targets.iter().enumerate() {
                        if let Some(&t_coef) = target.coefficients.get(item)
                            && t_coef != 0.0
                        {
                            let row = target_rows[t_idx];
                            let r_idx = self.target_rows[t_idx].1;
                            let entry = acc.entry(r_idx).or_insert((row, 0.0));
                            entry.1 += t_coef * coef;
                        }
                    }
                } else if let Some(&row) = item_rows.get(item)
                    && let Some(&(_, r_idx)) = self.item_to_row.get(item)
                {
                    let entry = acc.entry(r_idx).or_insert((row, 0.0));
                    entry.1 += coef;
                }
            }
            let factors: Vec<(Row, f64)> = acc.into_values().filter(|(_, v)| *v != 0.0).collect();
            pb.add_column(stored.cost, 0.0.., factors);
        }

        let mut model = pb
            .try_optimise(Sense::Minimise)
            .map_err(|e| ResolutionError::Str(format!("HiGHS rebuild failed: {e:?}")))?;
        model.make_quiet();
        Ok(model)
    }

    /// 求解当前 RMP（Phase II）。
    /// 小规模模型直接复用热启动基；超大规模模型（≥ 25,000 列）或热启动报错时，
    /// 自动重建干净模型以触发 HiGHS 完整 presolve。
    pub fn solve_phase2(&mut self) -> Result<MasterSolveState<I, F>, ResolutionError> {
        let use_fresh_presolve = self.total_cols >= 25_000;
        let solved = if use_fresh_presolve {
            let mut model = self.rebuild_model()?;
            // 对超大规模多星球模型设置重试阶梯：默认单纯形超时（高度退化循环）时，
            // 依次切换 Devex 对偶边权 + 随机扰动种子、Equilibration 缩放破环。
            let _ = model.try_set_option("time_limit", 10.0_f64);
            let mut attempt = model
                .try_solve()
                .map_err(|e| ResolutionError::Str(format!("HiGHS presolved solve error: {e:?}")))?;
            if attempt.status() == HighsModelStatus::ReachedTimeLimit {
                let mut m2 = self.rebuild_model()?;
                let _ = m2.try_set_option("simplex_scale_strategy", 1_i32);
                let _ = m2.try_set_option("random_seed", 42_i32);
                let _ = m2.try_set_option("time_limit", 25.0_f64);
                attempt = m2.try_solve().map_err(|e| {
                    ResolutionError::Str(format!("HiGHS scale fallback solve error: {e:?}"))
                })?;
            }
            if attempt.status() == HighsModelStatus::ReachedTimeLimit {
                let mut m3 = self.rebuild_model()?;
                let _ = m3.try_set_option("simplex_dual_edge_weight_strategy", 1_i32);
                let _ = m3.try_set_option("random_seed", 7_i32);
                let _ = m3.try_set_option("time_limit", 45.0_f64);
                attempt = m3.try_solve().map_err(|e| {
                    ResolutionError::Str(format!("HiGHS Devex fallback solve error: {e:?}"))
                })?;
            }
            attempt
        } else {
            let model = self
                .model
                .take()
                .expect("HighsColumnGenMaster model missing");
            match model.try_solve() {
                Ok(s) => s,
                Err(_) => {
                    let fresh = self.rebuild_model()?;
                    fresh.try_solve().map_err(|e| {
                        ResolutionError::Str(format!("HiGHS fallback solve error: {e:?}"))
                    })?
                }
            }
        };
        let status = solved.status();
        let result = match status {
            HighsModelStatus::Optimal => {
                let cost = solved.objective_value();
                let sol = solved.get_solution();
                let col_vals = sol.columns();
                let row_duals = sol.dual_rows();

                let mut primal_flows: AIndexMap<F, f64> =
                    AIndexMap::with_capacity_and_hasher(self.flows.len(), Default::default());
                for stored in &self.flows {
                    let val = col_vals.get(stored.col_index).copied().unwrap_or(0.0);
                    primal_flows.insert(stored.id.clone(), val);
                }
                let effective_duals = self.extract_effective_duals(row_duals);
                Ok(MasterSolveState::Optimal {
                    cost,
                    primal_flows,
                    effective_duals,
                    num_cols: self.total_cols,
                    num_rows: self.total_rows,
                })
            }
            HighsModelStatus::Infeasible => Ok(MasterSolveState::Infeasible),
            HighsModelStatus::Unbounded | HighsModelStatus::UnboundedOrInfeasible => {
                Ok(MasterSolveState::Unbounded)
            }
            other => Err(ResolutionError::Str(format!(
                "HiGHS returned status: {other:?}"
            ))),
        };
        if !use_fresh_presolve {
            self.model = Some(Model::from(solved));
        }
        result
    }

    /// 构造并求解 Phase I 辅助问题（所有真实列成本为 0，每行引入非负人工变量且成本为 1）。
    pub fn solve_phase1(&self) -> Result<Phase1State<I>, ResolutionError> {
        let mut pb = ColProblem::new();
        let mut total_rows = 0usize;

        let mut p1_item_rows: AIndexMap<I, (Row, usize)> = AIndexMap::default();
        for item in self.item_to_row.keys() {
            let row = if self.strict_sink {
                pb.add_row(0.0..=0.0)
            } else {
                pb.add_row(0.0..)
            };
            p1_item_rows.insert(item.clone(), (row, total_rows));
            total_rows += 1;
        }

        let mut p1_target_rows: Vec<(Row, usize)> = Vec::with_capacity(self.targets.len());
        for target in &self.targets {
            let row = pb.add_row(target.constant..=target.constant);
            p1_target_rows.push((row, total_rows));
            total_rows += 1;
        }

        let collect_item_factors = |item: &I, sign: f64| -> Vec<(Row, f64)> {
            if self.unconstrained.contains(item) {
                return Vec::new();
            }
            if self.item_in_targets.contains(item) {
                let mut acc: AIndexMap<usize, (Row, f64)> = AIndexMap::default();
                for (t_idx, target) in self.targets.iter().enumerate() {
                    if let Some(&t_coef) = target.coefficients.get(item)
                        && t_coef != 0.0
                    {
                        let (row, r_idx) = p1_target_rows[t_idx];
                        let entry = acc.entry(r_idx).or_insert((row, 0.0));
                        entry.1 += sign * t_coef;
                    }
                }
                acc.into_values().filter(|(_, v)| *v != 0.0).collect()
            } else if let Some(&(row, _)) = p1_item_rows.get(item) {
                vec![(row, sign)]
            } else {
                Vec::new()
            }
        };

        // 1) 真实列：成本全部为 0
        for item in self.sources.keys() {
            pb.add_column(0.0, 0.0.., collect_item_factors(item, 1.0));
        }
        for item in self.sinks.keys() {
            pb.add_column(0.0, 0.0.., collect_item_factors(item, -1.0));
        }
        for stored in &self.flows {
            let mut acc: AIndexMap<usize, (Row, f64)> = AIndexMap::default();
            for (item, &coef) in &stored.coefficients {
                if coef == 0.0 || self.unconstrained.contains(item) {
                    continue;
                }
                if self.item_in_targets.contains(item) {
                    for (t_idx, target) in self.targets.iter().enumerate() {
                        if let Some(&t_coef) = target.coefficients.get(item)
                            && t_coef != 0.0
                        {
                            let (row, r_idx) = p1_target_rows[t_idx];
                            let entry = acc.entry(r_idx).or_insert((row, 0.0));
                            entry.1 += t_coef * coef;
                        }
                    }
                } else if let Some(&(row, r_idx)) = p1_item_rows.get(item) {
                    let entry = acc.entry(r_idx).or_insert((row, 0.0));
                    entry.1 += coef;
                }
            }
            let factors: Vec<(Row, f64)> = acc.into_values().filter(|(_, v)| *v != 0.0).collect();
            pb.add_column(0.0, 0.0.., factors);
        }

        // 2) 人工变量：成本全部为 1.0
        for &(row, _) in p1_item_rows.values() {
            pb.add_column(1.0, 0.0.., [(row, 1.0)]);
            if self.strict_sink {
                pb.add_column(1.0, 0.0.., [(row, -1.0)]);
            }
        }
        for &(row, _) in &p1_target_rows {
            pb.add_column(1.0, 0.0.., [(row, 1.0)]);
            pb.add_column(1.0, 0.0.., [(row, -1.0)]);
        }

        let mut model = pb
            .try_optimise(Sense::Minimise)
            .map_err(|e| ResolutionError::Str(format!("HiGHS Phase I init failed: {e:?}")))?;
        model.make_quiet();
        let solved = model.try_solve().map_err(|e| ResolutionError::Str(format!("HiGHS solve error: {e:?}")))?;
        if solved.status() != HighsModelStatus::Optimal {
            return Err(ResolutionError::Str(format!(
                "HiGHS Phase I unexpected status: {:?}",
                solved.status()
            )));
        }
        let artificial_sum = solved.objective_value();
        let sol = solved.get_solution();
        let row_duals = sol.dual_rows();

        let mut effective_duals: AIndexMap<I, f64> = AIndexMap::default();
        for (item, &(_, r_idx)) in &p1_item_rows {
            let d = row_duals.get(r_idx).copied().unwrap_or(0.0);
            if d != 0.0 {
                effective_duals.insert(item.clone(), d);
            }
        }
        for (t_idx, target) in self.targets.iter().enumerate() {
            let (_, r_idx) = p1_target_rows[t_idx];
            let td = row_duals.get(r_idx).copied().unwrap_or(0.0);
            if td == 0.0 {
                continue;
            }
            for (item, &t_coef) in &target.coefficients {
                if t_coef != 0.0 {
                    *effective_duals.entry(item.clone()).or_insert(0.0) += t_coef * td;
                }
            }
        }

        Ok(Phase1State {
            artificial_sum,
            effective_duals,
        })
    }

    fn extract_effective_duals(&self, row_duals: &[f64]) -> AIndexMap<I, f64> {
        let mut effective: AIndexMap<I, f64> = AIndexMap::default();
        for (item, &(_, r_idx)) in &self.item_to_row {
            let d = row_duals.get(r_idx).copied().unwrap_or(0.0);
            if d != 0.0 {
                effective.insert(item.clone(), d);
            }
        }
        for (t_idx, target) in self.targets.iter().enumerate() {
            let (_, r_idx) = self.target_rows[t_idx];
            let td = row_duals.get(r_idx).copied().unwrap_or(0.0);
            if td == 0.0 {
                continue;
            }
            for (item, &t_coef) in &target.coefficients {
                if t_coef != 0.0 {
                    *effective.entry(item.clone()).or_insert(0.0) += t_coef * td;
                }
            }
        }
        effective
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase2_dual_sign_and_incremental_add_col() {
        let mut flows: AIndexMap<u32, (Flow<&'static str>, f64)> = AIndexMap::default();
        let mut slow: Flow<&'static str> = AIndexMap::default();
        slow.insert("A", -1.0);
        slow.insert("B", 1.0);
        flows.insert(1, (slow, 10.0));

        let mut target: Flow<&'static str> = AIndexMap::default();
        target.insert("B", 1.0);

        let mut data = SolverData::new_simple(target, flows);
        data.sources.insert("A", 1.0);
        data.strict_source = true;

        let mut master = HighsColumnGenMaster::from_seed(&data).unwrap();
        let state = master.solve_phase2().unwrap();
        let MasterSolveState::Optimal {
            cost,
            effective_duals,
            ..
        } = state
        else {
            panic!("expected Optimal");
        };
        assert!((cost - 11.0).abs() < 1e-9, "cost={cost}");
        let pi_a = effective_duals.get(&"A").copied().unwrap_or(0.0);
        let pi_b = effective_duals.get(&"B").copied().unwrap_or(0.0);
        assert!((pi_a - 1.0).abs() < 1e-9, "pi_a={pi_a}");
        assert!((pi_b - 11.0).abs() < 1e-9, "pi_b={pi_b}");

        let rc = 2.0 - (pi_b * 1.0 + pi_a * (-1.0));
        assert!((rc - (-8.0)).abs() < 1e-9, "rc={rc}");

        let mut fast: Flow<&'static str> = AIndexMap::default();
        fast.insert("A", -1.0);
        fast.insert("B", 1.0);
        master.add_flow(2, fast, 2.0);

        let state2 = master.solve_phase2().unwrap();
        let MasterSolveState::Optimal { cost: cost2, .. } = state2 else {
            panic!("expected Optimal after add_flow");
        };
        assert!((cost2 - 3.0).abs() < 1e-9, "cost2={cost2}");
    }

    #[test]
    fn phase1_recovers_missing_quality_tier_like_1000_percent_mod() {
        let mut flows: AIndexMap<u32, (Flow<&'static str>, f64)> = AIndexMap::default();
        let mut empty_slot: Flow<&'static str> = AIndexMap::default();
        empty_slot.insert("raw", -1.0);
        empty_slot.insert("A", 1.0);
        flows.insert(1, (empty_slot, 1.0));

        let mut pure_qual: Flow<&'static str> = AIndexMap::default();
        pure_qual.insert("raw", -1.0);
        pure_qual.insert("C", 1.0);
        flows.insert(2, (pure_qual, 1.0));

        let mut target: Flow<&'static str> = AIndexMap::default();
        target.insert("B", 1.0);

        let mut data = SolverData::new_simple(target, flows);
        data.sources.insert("raw", 1.0);
        data.strict_source = true;

        let mut master = HighsColumnGenMaster::from_seed(&data).unwrap();
        assert!(matches!(
            master.solve_phase2().unwrap(),
            MasterSolveState::Infeasible
        ));

        let p1 = master.solve_phase1().unwrap();
        assert!((p1.artificial_sum - 1.0).abs() < 1e-9);
        let y_b = p1.effective_duals.get(&"B").copied().unwrap_or(0.0);
        assert!((y_b - 1.0).abs() < 1e-9, "y_b={y_b}");

        let y_raw = p1.effective_duals.get(&"raw").copied().unwrap_or(0.0);
        let y_c = p1.effective_duals.get(&"C").copied().unwrap_or(0.0);
        let rc_p1 = 0.0 - (y_b * 0.1 + y_c * 0.9 + y_raw * (-1.0));
        assert!(rc_p1 < -1e-6, "Phase I reduced cost should be negative: {rc_p1}");

        let mut mixed: Flow<&'static str> = AIndexMap::default();
        mixed.insert("raw", -1.0);
        mixed.insert("B", 0.1);
        mixed.insert("C", 0.9);
        master.add_flow(3, mixed, 1.0);

        let p1_after = master.solve_phase1().unwrap();
        assert!(p1_after.artificial_sum < 1e-9, "now feasible");

        let state2 = master.solve_phase2().unwrap();
        let MasterSolveState::Optimal { cost, .. } = state2 else {
            panic!("expected Optimal");
        };
        assert!((cost - 20.0).abs() < 1e-9, "cost={cost}");
    }
}
