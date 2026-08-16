use good_lp::{IntoAffineExpression, Solution, variable};

use crate::concept::{AIndexMap, AIndexSet, Flow, ItemIdent};
use crate::ruiz::RuizSolver;
use core::f64;

use std::fmt::Debug;
use std::hash::Hash;
use std::sync::mpsc::*;
use std::time::Instant;

#[must_use]
pub fn flow_add<T>(a: &Flow<T>, b: &Flow<T>, c: f64) -> Flow<T>
where
    T: Eq + Hash + Clone,
{
    let mut result = a.clone();
    for (key, value) in b {
        let entry = result.entry(key.clone()).or_insert(0.0);
        *entry += value * c;
    }
    result
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TargetSpec<I: ItemIdent> {
    pub constant: f64,
    pub coefficients: Flow<I>,
}

#[derive(Debug, Clone)]
pub struct FlowSpec<I: ItemIdent> {
    pub coefficients: Flow<I>,
    pub cost: f64,
    pub fixed: Option<f64>, // 如果是Some(v)，表示这个原始变量必须是v
}

#[derive(Debug, Clone)]
pub struct SolverData<I, R>
where
    I: ItemIdent,
    R: ItemIdent,
{
    pub target: Vec<TargetSpec<I>>,
    pub flows: AIndexMap<R, FlowSpec<I>>,
    pub sources: Flow<I>, //  输入特定物品消耗的价值
    pub sinks: Flow<I>,   //  产生额外物品的惩罚
    // 我还不知道怎么称呼，目前规定如下：
    // 如果是严格模式，相比普通模式有如下限制：只能使用来自external的输入
    pub strict_source: bool,
    // 如果是严格模式，相比普通模式有如下限制：没有出现在target中的物品必须配平
    pub strict_sink: bool,
}

// TODO: warning: large size difference between variants
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum SolverSolution<I, R> {
    Solved {
        prim: Flow<R>,
        dual: Option<Flow<I>>,
        prim_scale: Flow<R>,
        dual_scale: Flow<I>,
        global_scale: f64,
        sum: Flow<I>,
        cost: f64,
    },
    NotSolved {
        no_provider: Vec<I>,
        no_consumer: Vec<I>,
        description: String,
    },
}

impl<I, R> Default for SolverSolution<I, R>
where
    I: ItemIdent,
    R: ItemIdent,
{
    fn default() -> Self {
        SolverSolution::NotSolved {
            no_provider: vec![],
            no_consumer: vec![],
            description: "未求解".to_string(),
        }
    }
}

impl<I, R> SolverSolution<I, R>
where
    I: ItemIdent,
    R: ItemIdent,
{
    pub fn get_prim_raw_of(&self, i: &R) -> Option<f64> {
        match self {
            SolverSolution::Solved {
                prim,
                prim_scale,
                global_scale,
                ..
            } => match (prim.get(i), prim_scale.get(i)) {
                (Some(v), Some(s)) => Some(*v / s * global_scale),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn get_prim_of(&self, i: &R) -> Option<f64> {
        match self {
            SolverSolution::Solved { prim, .. } => prim.get(i).cloned(),
            _ => None,
        }
    }

    pub fn get_dual_raw_of_of(&self, i: &I) -> Option<f64> {
        match self {
            SolverSolution::Solved {
                dual: Some(dual),
                dual_scale,
                ..
            } => dual
                .get(i)
                .cloned()
                .map(|v| v * dual_scale.get(i).cloned().unwrap_or(1.0)),

            _ => None,
        }
    }

    pub fn get_dual_of(&self, i: &I) -> Option<f64> {
        match self {
            SolverSolution::Solved {
                dual: Some(dual), ..
            } => dual.get(i).cloned(),
            _ => None,
        }
    }

    pub fn get_cost(&self) -> Option<f64> {
        match self {
            SolverSolution::Solved { cost, .. } => Some(*cost),
            _ => None,
        }
    }

    pub fn get_sum(&self) -> Option<&Flow<I>> {
        match self {
            SolverSolution::Solved { sum, .. } => Some(sum),
            _ => None,
        }
    }

    pub fn get_sum_of(&self, i: &I) -> Option<f64> {
        match self {
            SolverSolution::Solved { sum, .. } => sum.get(i).cloned(),
            _ => None,
        }
    }

    pub fn get_sum_raw_of(&self, i: &I) -> Option<f64> {
        match self {
            SolverSolution::Solved {
                sum,
                dual_scale,
                global_scale,
                ..
            } => sum
                .get(i)
                .map(|v| *v * dual_scale.get(i).cloned().unwrap_or(1.0) * global_scale),
            _ => None,
        }
    }
}

impl<I, R> SolverData<I, R>
where
    I: ItemIdent,
    R: ItemIdent,
{
    pub fn new_simple(target: Flow<I>, flows: AIndexMap<R, (Flow<I>, f64)>) -> Self {
        Self {
            target: target
                .into_iter()
                .map(|(item_id, constant)| TargetSpec {
                    constant,
                    coefficients: [(item_id, 1.0)].into_iter().collect(),
                })
                .collect(),
            flows: flows
                .into_iter()
                .map(|(flow_id, (coefficients, cost))| {
                    (
                        flow_id,
                        FlowSpec {
                            coefficients,
                            cost,
                            fixed: None,
                        },
                    )
                })
                .collect(),
            sources: AIndexMap::default(),
            sinks: AIndexMap::default(),
            strict_source: false,
            strict_sink: false,
        }
    }

    pub fn with_sources(mut self, sources: Flow<I>) -> Self {
        self.sources.extend(sources);
        self
    }

    pub fn with_sinks(mut self, sinks: Flow<I>) -> Self {
        self.sinks.extend(sinks);
        self
    }

    pub fn with_strict_source(mut self, strict: bool) -> Self {
        self.strict_source = strict;
        self
    }

    pub fn with_strict_sink(mut self, strict: bool) -> Self {
        self.strict_sink = strict;
        self
    }

    pub fn trim_flows(&mut self) -> bool {
        let mut changed = false;
        if self.strict_source {
            // 在strict_source模式下，移除所有无法使用的配方
            let instant = std::time::Instant::now();
            let mut status = AIndexMap::default();
            enum ItemStatus<R> {
                Pending {
                    providers: AIndexSet<R>,
                    consumers: AIndexSet<R>,
                },
                Usable,
            }

            for (
                f_id,
                FlowSpec {
                    coefficients,
                    cost: _,
                    fixed: _,
                },
            ) in &self.flows
            {
                for (item_id, &amount) in coefficients {
                    let entry =
                        status
                            .entry(item_id.clone())
                            .or_insert_with(|| ItemStatus::Pending {
                                providers: AIndexSet::default(),
                                consumers: AIndexSet::default(),
                            });
                    if amount > 0.0 {
                        // 生产这个物品的配方
                        match entry {
                            ItemStatus::Pending { providers, .. } => {
                                providers.insert(f_id.clone());
                            }
                            ItemStatus::Usable => {}
                        }
                    }
                    if amount < 0.0 {
                        // 消耗这个物品的配方
                        match entry {
                            ItemStatus::Pending { consumers, .. } => {
                                consumers.insert(f_id.clone());
                            }
                            ItemStatus::Usable => {}
                        }
                    }
                    match entry {
                        ItemStatus::Pending {
                            providers,
                            consumers,
                        } if !providers.is_empty() && !consumers.is_empty() => {
                            *entry = ItemStatus::Usable;
                        }
                        _ => {}
                    }
                }
            }

            let before = self.flows.len();

            let needed_by_target = self.target.iter().fold(
                AIndexSet::default(),
                |mut acc,
                 TargetSpec {
                     constant,
                     coefficients,
                 }| {
                    for (i_id, coef) in coefficients {
                        if *coef * constant > 0.0 {
                            // 系数与常数同号，说明目标需要这个物品
                            acc.insert(i_id.clone());
                        }
                    }
                    acc
                },
            );

            for (i_id, entry) in &status {
                if let ItemStatus::Pending {
                    providers,
                    consumers,
                } = entry
                    && providers.is_empty() // 没有生产这个物品的配方
                        && !self.sources.contains_key(i_id) // 外部也不能提供
                        && !needed_by_target.contains(i_id)
                // 目标也不需要
                {
                    // log::debug!(
                    //     "求解器：物品 {:?} 无法获得，移除相关配方 {} 个",
                    //     i_id,
                    //     providers.len() + consumers.len()
                    // );
                    for f_id in consumers {
                        self.flows.swap_remove(f_id);
                        changed = true;
                    }
                }
            }

            let after = self.flows.len();
            if before != after {
                log::debug!(
                    "求解器：移除了 {} 个无法使用的配方 ({} -> {})",
                    before - after,
                    before,
                    after
                );
            }
            log::debug!(
                "求解器：移除无法使用的配方耗时 {} ms",
                instant.elapsed().as_millis()
            );
        }
        changed
    }

    pub fn solve(mut self) -> SolverSolution<I, R> {
        if self.flows.is_empty() {
            return SolverSolution::NotSolved {
                no_provider: vec![],
                no_consumer: vec![],
                description: "没有可用的配方".to_string(),
            };
        }
        if self.target.is_empty() {
            return SolverSolution::NotSolved {
                no_provider: vec![],
                no_consumer: vec![],
                description: "没有目标物品。".to_string(),
            };
        }

        log::info!("求解器：开始剪枝");
        let mut count = 0;
        let instant = Instant::now();
        let len_before = self.flows.len();
        while self.trim_flows() {
            count += 1;
        }
        let len_after = self.flows.len();
        log::info!(
            "求解器：剪枝完成，共执行了 {} 次剪枝操作，移除了 {} 个配方 ({} -> {})，耗时 {:.2?}",
            count,
            len_before - len_after,
            len_before,
            len_after,
            instant.elapsed()
        );

        let mut problem_variables = good_lp::ProblemVariables::new();
        let mut item_in_targets = AIndexSet::default();
        for target in &self.target {
            for (i_id, _coef) in target.coefficients.iter() {
                item_in_targets.insert(i_id.clone());
            }
        }
        // 用户提供的流编号 -> 变量的映射
        let mut flow_vars = AIndexMap::default();
        // 物品源变量
        let mut source_vars = AIndexMap::default();
        // 物品汇变量
        let mut sink_vars = AIndexMap::default();
        for f_id in self.flows.keys() {
            let var = problem_variables.add(variable().min(0));
            flow_vars.insert(f_id.clone(), var);
        }

        let mut item_balances = AIndexMap::default();

        log::info!(
            "求解器：开始构建物品平衡表达式：一共有 {} 个配方变量",
            self.flows.len()
        );

        // 因为存在0开销转换流，必须限制产物为0.
        // 目前约定的0开销转换流都表示其转换在其他建筑中隐式完成，所以不消耗代价，同理也必须完全配平，不允许有剩余。
        let mut force_zero_items = AIndexSet::default();
        for (f_id, flow_spec) in &self.flows {
            let var = flow_vars.get(f_id).unwrap();
            for (item_id, &amount) in &flow_spec.coefficients {
                let entry = item_balances
                    .entry(item_id.clone())
                    .or_insert(good_lp::Expression::from(0.0));
                let val = amount;

                *entry += val * *var;
                if flow_spec.cost == 0.0 && amount > 0.0 {
                    force_zero_items.insert(item_id.clone());
                }
            }
        }
        log::info!("求解器：一共有 {} 个物品需要平衡", item_balances.len(),);

        for (item_id, _) in &self.sources {
            let var = problem_variables.add(variable().min(0));
            source_vars.insert(item_id.clone(), var);
            let entry = item_balances
                .entry(item_id.clone())
                .or_insert(good_lp::Expression::from(0.0));
            *entry += 1.0 * var;
        }
        for (item_id, _) in &self.sinks {
            let var = problem_variables.add(variable().min(0));
            sink_vars.insert(item_id.clone(), var);
            let entry = item_balances
                .entry(item_id.clone())
                .or_insert(good_lp::Expression::from(0.0));
            *entry -= 1.0 * var;
        }
        let mut no_providers: AIndexSet<I> = item_balances.keys().cloned().collect();
        let mut no_consumers: AIndexSet<I> = item_balances.keys().cloned().collect();
        for (_flow, flow_spec) in &self.flows {
            for (item_id, &amount) in &flow_spec.coefficients {
                if amount > 0.0 {
                    no_providers.swap_remove(item_id);
                }
                if amount < 0.0 {
                    no_consumers.swap_remove(item_id);
                }
            }
        }
        for item in self.sources.keys() {
            no_providers.swap_remove(item);
        }
        for item in self.sinks.keys() {
            no_consumers.swap_remove(item);
        }
        let mut constraints = Vec::new();
        let mut item_to_constraint = AIndexMap::default();
        let mut add_constraint = |item_id: &I, constraint: good_lp::Constraint| {
            constraints.push(constraint);
            item_to_constraint.insert(item_id.clone(), constraints.len() - 1);
        };
        for (item_id, expr) in &item_balances {
            // 所有目标都间接转移了，不再在此处做判断
            {
                // 严格模式下，不能凭空输入。非严格模式下，有来源的物品不能有凭空输入。
                // 非目标物品，不能为负
                if force_zero_items.contains(item_id) {
                    add_constraint(item_id, expr.clone().eq(0.0));
                    continue;
                }
                if self.strict_source {
                    // 不能从外部借用
                    if self.strict_sink {
                        // 必须配平
                        add_constraint(item_id, expr.clone().eq(0.0));
                    } else {
                        // 不用配平
                        add_constraint(item_id, expr.clone().geq(0.0));
                    }
                } else if no_providers.contains(item_id) {
                    // 需要借用，不用限制
                } else if self.strict_sink {
                    // 必须配平
                    add_constraint(item_id, expr.clone().eq(0.0));
                } else {
                    // 不用配平
                    add_constraint(item_id, expr.clone().geq(0.0));
                }
            }
        }
        for source_var in source_vars.values() {
            constraints.push(source_var.into_expression().geq(0.0));
        }
        // 添加求解目标的限制

        let mut target_exprs = vec![good_lp::Expression::from(0.0); self.target.len()];
        for item in &item_in_targets {
            for (target_idx, target) in self.target.iter().enumerate() {
                if let Some(&coef) = target.coefficients.get(item) {
                    if coef == 0.0 {
                        continue;
                    }
                    // 分离数量级平衡和问题构造后就成平凡的了
                    target_exprs[target_idx] += coef
                        * item_balances
                            .get(item)
                            .cloned()
                            .unwrap_or(good_lp::Expression::from(0.0));
                }
            }
        }
        for (t_idx, target) in self.target.iter().enumerate() {
            let target_expr = &target_exprs[t_idx];
            let constant = target.constant;
            constraints.push(target_expr.clone().eq(constant));
        }
        let mut optimization_expr = good_lp::Expression::from(0.0);
        for (flow_id, flow_spec) in &self.flows {
            let var = flow_vars.get(flow_id).unwrap();
            optimization_expr += flow_spec.cost * *var;
        }
        for (item_id, cost) in &self.sources {
            let var = source_vars.get(item_id).unwrap();
            optimization_expr += *cost * *var;
        }
        for (item_id, cost) in &self.sinks {
            let var = sink_vars.get(item_id).unwrap();
            optimization_expr += *cost * *var;
        }
        if !no_providers.is_empty() {
            log::warn!("没有来源的物品：{:?}个", no_providers.len());
        }
        if !no_consumers.is_empty() {
            log::warn!("没有去处的物品：{:?}个", no_consumers.len());
        }
        if constraints.len() < 8 {
            log::debug!("求解器：构建的约束表达式: {:?}", constraints);

            log::debug!("求解器：对应流变量: {:?}", flow_vars);
        }
        let solution =
            RuizSolver::new(optimization_expr.clone(), constraints, problem_variables).solve();

        match solution {
            Ok(sol) => {
                log::info!("求解器：求解成功，开始构建结果");
                let global_scale = sol.global_scale;
                let mut sum = Flow::default();
                let mut prim = Flow::default();
                let mut prim_scale = Flow::default();

                for (f_id, var) in &flow_vars {
                    let cur_prim_scale = sol.prim_scales.get(var).cloned().unwrap_or(1.0);
                    let value = sol.inner.value(*var) * cur_prim_scale / global_scale;

                    prim.insert(f_id.clone(), value);

                    prim_scale.insert(f_id.clone(), cur_prim_scale);
                    for (item_id, &amount) in &self.flows[f_id].coefficients {
                        let entry = sum.entry(item_id.clone()).or_insert(0.0);
                        *entry += amount * value;
                    }
                }
                SolverSolution::Solved {
                    prim,
                    prim_scale,
                    dual: None,
                    dual_scale: item_to_constraint
                        .iter()
                        .map(|(item_id, &c_idx)| {
                            let dual_scale = sol.dual_scales[c_idx];
                            (item_id.clone(), dual_scale)
                        })
                        .collect(),
                    sum,
                    cost: sol.cost,
                    global_scale,
                }
            }
            Err(err) => {
                log::error!("求解器：求解失败，错误信息: {:?}", err);
                let err_string = match err {
                    good_lp::ResolutionError::Unbounded => "求解无界（目标可无限增大）".to_string(),
                    good_lp::ResolutionError::Infeasible => "无可行解（目标不可达）".to_string(),
                    good_lp::ResolutionError::Other(_) => "求解器错误".to_string(),
                    good_lp::ResolutionError::Str(s) => format!("求解器错误: {s}"),
                };
                SolverSolution::NotSolved {
                    no_provider: no_providers.iter().cloned().collect(),
                    no_consumer: no_consumers.iter().cloned().collect(),
                    description: err_string,
                }
            }
        }
    }

    pub fn make_dedicated_solver_thread(
        solution_tx: Sender<SolverSolution<I, R>>,
        problem_rx: Receiver<SolverData<I, R>>,
    ) {
        std::thread::spawn(move || {
            log::info!("求解线程启动");
            loop {
                let mut last_req = match problem_rx.recv() {
                    Ok(req) => req,
                    Err(_) => break,
                };
                // 尽可能多地丢弃后续请求，只保留最新
                while let Ok(req) = problem_rx.try_recv() {
                    // 虽然不太可能，因为每次算都很快。
                    log::info!("丢弃了一个过时的求解请求");

                    last_req = req;
                }
                if solution_tx.send(last_req.solve()).is_err() {
                    // 接收方已关闭，退出线程
                    break;
                }
            }
            log::info!("求解线程退出");
        });
    }

    pub fn make_solver_thread(
        solution_tx: Sender<(usize, SolverSolution<I, R>)>,
        problem_rx: Receiver<(usize, SolverData<I, R>)>,
    ) {
        std::thread::spawn(move || {
            log::info!("求解线程启动");
            loop {
                let mut reqs = AIndexMap::default();
                std::thread::sleep(std::time::Duration::from_millis(50));
                while let Ok((req_id, req)) = problem_rx.try_recv() {
                    reqs.insert(req_id, req);
                }
                for (req_id, req) in reqs.into_iter() {
                    let result = req.solve();

                    if solution_tx.send((req_id, result)).is_err() {
                        // 接收方已关闭，退出线程
                        log::info!("求解线程退出");
                        break;
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最简单的冶炼：1 铁矿 → 1 铁板，成本 1；目标产出 1 铁板
    fn smelting_problem() -> SolverData<&'static str, &'static str> {
        let mut target = AIndexMap::default();
        target.insert("iron-plate", 1.0);

        let mut flows = AIndexMap::default();
        let mut smelt = AIndexMap::default();
        smelt.insert("iron-ore", -1.0);
        smelt.insert("iron-plate", 1.0);
        flows.insert("smelt", (smelt, 1.0));

        SolverData::new_simple(target, flows)
    }

    #[test]
    fn solve_simple_recipe() {
        let solution = smelting_problem().solve();
        match solution {
            SolverSolution::Solved {
                prim, sum, cost, ..
            } => {
                assert!((prim["smelt"] - 1.0).abs() < 1e-6, "prim: {prim:?}");
                assert!((sum["iron-plate"] - 1.0).abs() < 1e-6, "sum: {sum:?}");
                assert!((sum["iron-ore"] + 1.0).abs() < 1e-6, "sum: {sum:?}");
                assert!((cost - 1.0).abs() < 1e-6, "cost: {cost}");
            }
            SolverSolution::NotSolved { description, .. } => {
                panic!("求解失败: {description}")
            }
        }
    }

    #[test]
    fn flow_add_combines_with_scale() {
        let mut a = AIndexMap::default();
        a.insert("x", 1.0);
        a.insert("y", 2.0);
        let mut b = AIndexMap::default();
        b.insert("y", 3.0);
        b.insert("z", 1.0);
        let r = flow_add(&a, &b, 2.0);
        assert_eq!(r.len(), 3);
        assert!((r["x"] - 1.0).abs() < 1e-9);
        assert!((r["y"] - 8.0).abs() < 1e-9);
        assert!((r["z"] - 2.0).abs() < 1e-9);
    }

    #[test]
    fn flow_add_empty_base_and_negative_scale() {
        let a = AIndexMap::default();
        let mut b = AIndexMap::default();
        b.insert("x", 4.0);
        let r = flow_add(&a, &b, -0.5);
        assert!((r["x"] + 2.0).abs() < 1e-9);
    }

    #[test]
    fn solve_chooses_cheapest_recipe() {
        // 两条配方产出同一物品，成本不同：求解器应选择成本低的
        let mut target = AIndexMap::default();
        target.insert("iron-plate", 1.0);
        let mut flows = AIndexMap::default();
        let mut cheap = AIndexMap::default();
        cheap.insert("iron-ore", -1.0);
        cheap.insert("iron-plate", 1.0);
        let mut expensive = AIndexMap::default();
        expensive.insert("iron-ore", -2.0);
        expensive.insert("iron-plate", 1.0);
        flows.insert("cheap", (cheap, 1.0));
        flows.insert("expensive", (expensive, 5.0));

        let solution = SolverData::new_simple(target, flows).solve();
        match solution {
            SolverSolution::Solved { prim, cost, .. } => {
                assert!(
                    (prim["cheap"] - 1.0).abs() < 1e-5,
                    "应选择低成本配方: {prim:?}"
                );
                assert!(
                    (prim["expensive"]).abs() < 1e-5,
                    "不应选择高成本配方: {prim:?}"
                );
                assert!((cost - 1.0).abs() < 1e-5, "总成本应为 1: {cost}");
            }
            SolverSolution::NotSolved { description, .. } => panic!("求解失败: {description}"),
        }
    }

    #[test]
    fn solve_with_sources_provides_missing_input() {
        // ore 无配方生产，由外部 source 提供；source 变量计入成本
        let mut target = AIndexMap::default();
        target.insert("iron-plate", 1.0);
        let mut flows = AIndexMap::default();
        let mut smelt = AIndexMap::default();
        smelt.insert("iron-ore", -1.0);
        smelt.insert("iron-plate", 1.0);
        flows.insert("smelt", (smelt, 1.0));

        let mut sources = AIndexMap::default();
        sources.insert("iron-ore", 2.0); // 每单位 ore 输入价值 2

        let solution = SolverData::new_simple(target, flows)
            .with_sources(sources)
            .solve();
        match solution {
            SolverSolution::Solved { sum, cost, .. } => {
                assert!((sum["iron-ore"] + 1.0).abs() < 1e-5, "消耗 1 ore: {sum:?}");
                assert!((cost - 3.0).abs() < 1e-5, "成本 = 1 ore * 3: {cost}");
            }
            SolverSolution::NotSolved { description, .. } => panic!("求解失败: {description}"),
        }
    }

    #[test]
    fn solve_negative_target_means_consume() {
        // target 为负 = 必须净消耗该物品（等式约束）
        let mut target = AIndexMap::default();
        target.insert("iron-plate", -1.0);
        let mut flows = AIndexMap::default();
        let mut burn = AIndexMap::default();
        burn.insert("iron-plate", -1.0);
        flows.insert("burn", (burn, 2.0));

        let solution = SolverData::new_simple(target, flows).solve();
        match solution {
            SolverSolution::Solved { sum, cost, .. } => {
                assert!((sum["iron-plate"] + 1.0).abs() < 1e-5, "净消耗 1: {sum:?}");
                assert!((cost - 2.0).abs() < 1e-5, "成本 2: {cost}");
            }
            SolverSolution::NotSolved { description, .. } => panic!("求解失败: {description}"),
        }
    }

    #[test]
    fn solve_unreachable_target_reports_not_solved() {
        // 目标物品没有任何配方可以生产（也无 sources），LP 不可行
        let mut target = AIndexMap::default();
        target.insert("mystery", 1.0);
        let mut flows = AIndexMap::default();
        let mut smelt = AIndexMap::default();
        smelt.insert("iron-ore", -1.0);
        smelt.insert("iron-plate", 1.0);
        flows.insert("smelt", (smelt, 1.0));
        let solution = SolverData::new_simple(target, flows).solve();
        assert!(matches!(solution, SolverSolution::NotSolved { .. }));
    }

    #[test]
    fn solve_empty_flows_reports_no_recipe() {
        let mut target = AIndexMap::default();
        target.insert("iron-plate", 1.0);
        let flows: AIndexMap<&'static str, (AIndexMap<&'static str, f64>, f64)> =
            AIndexMap::default();
        let solution = SolverData::<&'static str, &'static str>::new_simple(target, flows).solve();
        assert!(matches!(solution, SolverSolution::NotSolved { .. }));
    }

    #[test]
    fn solve_empty_target_reports_no_target() {
        let target: AIndexMap<&'static str, f64> = AIndexMap::default();
        let flows: AIndexMap<&'static str, (AIndexMap<&'static str, f64>, f64)> =
            AIndexMap::default();
        let solution = SolverData::<&'static str, &'static str>::new_simple(target, flows).solve();
        assert!(matches!(solution, SolverSolution::NotSolved { .. }));
    }

    #[test]
    fn solve_zero_cost_flow_must_balance() {
        // 0 成本转换流的产出物必须完全配平：不能凭空产出目标
        let mut target = AIndexMap::default();
        target.insert("plate", 1.0);
        let mut flows = AIndexMap::default();
        let mut conv = AIndexMap::default();
        conv.insert("ore", -1.0);
        conv.insert("plate", 1.0);
        flows.insert("conv", (conv, 0.0)); // 0 成本
        let mut prod = AIndexMap::default();
        prod.insert("raw", -1.0);
        prod.insert("ore", 1.0);
        flows.insert("prod", (prod, 1.0));
        let solution = SolverData::new_simple(target, flows).solve();
        assert!(
            matches!(solution, SolverSolution::NotSolved { .. }),
            "0 成本转换流产出目标物品应不可行"
        );
    }

    #[test]
    fn solve_two_stage_recipe_chain() {
        // 两级配方：raw → ore → plate，中间物应配平
        let mut target = AIndexMap::default();
        target.insert("iron-plate", 1.0);
        let mut flows = AIndexMap::default();
        let mut mine = AIndexMap::default();
        mine.insert("raw-ore", -1.0);
        mine.insert("iron-ore", 1.0);
        flows.insert("mine", (mine, 1.0));
        let mut smelt = AIndexMap::default();
        smelt.insert("iron-ore", -1.0);
        smelt.insert("iron-plate", 1.0);
        flows.insert("smelt", (smelt, 2.0));

        let solution = SolverData::new_simple(target, flows).solve();
        match solution {
            SolverSolution::Solved { prim, sum, .. } => {
                assert!((prim["mine"] - 1.0).abs() < 1e-5, "prim: {prim:?}");
                assert!((prim["smelt"] - 1.0).abs() < 1e-5, "prim: {prim:?}");
                assert!((sum["raw-ore"] + 1.0).abs() < 1e-5, "sum: {sum:?}");
                assert!((sum["iron-ore"]).abs() < 1e-5, "中间物应配平: {sum:?}");
                assert!((sum["iron-plate"] - 1.0).abs() < 1e-5, "sum: {sum:?}");
            }
            SolverSolution::NotSolved { description, .. } => panic!("求解失败: {description}"),
        }
    }

    #[test]
    fn solve_accessors_consistent_with_fields() {
        let solution = smelting_problem().solve();
        match &solution {
            SolverSolution::Solved {
                prim, sum, cost, ..
            } => {
                assert_eq!(solution.get_prim_of(&"smelt"), prim.get(&"smelt").copied());
                assert_eq!(
                    solution.get_sum_of(&"iron-plate"),
                    sum.get(&"iron-plate").copied()
                );
                assert_eq!(solution.get_cost(), Some(*cost));
            }
            SolverSolution::NotSolved { description, .. } => panic!("求解失败: {description}"),
        }
    }

    #[test]
    fn trim_flows_removes_unusable_recipes_in_strict_mode() {
        // strict_source：孤立配方（消耗无法获得的物品且目标不需要）被剪掉
        let mut target = AIndexMap::default();
        target.insert("plate", 1.0);
        let mut flows = AIndexMap::default();
        let mut usable = AIndexMap::default();
        usable.insert("ore", -1.0);
        usable.insert("plate", 1.0);
        flows.insert("smelt", (usable, 1.0));
        let mut miner = AIndexMap::default();
        miner.insert("rock", -1.0);
        miner.insert("ore", 1.0);
        flows.insert("miner", (miner, 1.0));
        let mut orphan = AIndexMap::default();
        orphan.insert("uranium", -1.0); // 无法获得
        orphan.insert("waste", 1.0); // 目标不需要
        flows.insert("react", (orphan, 1.0));

        let mut sources = AIndexMap::default();
        sources.insert("rock", 1.0); // 叶子输入由外部提供

        let mut data = SolverData::new_simple(target, flows)
            .with_sources(sources)
            .with_strict_source(true);
        assert!(data.trim_flows());
        assert!(!data.flows.contains_key("react"));
        assert!(data.flows.contains_key("smelt"));
        assert!(data.flows.contains_key("miner"));
    }

    #[test]
    fn trim_flows_noop_without_strict_source() {
        // 非严格模式下 trim_flows 不剪枝
        let mut target = AIndexMap::default();
        target.insert("plate", 1.0);
        let mut flows = AIndexMap::default();
        let mut orphan = AIndexMap::default();
        orphan.insert("uranium", -1.0);
        orphan.insert("waste", 1.0);
        flows.insert("react", (orphan, 1.0));
        let mut data = SolverData::new_simple(target, flows);
        assert!(!data.trim_flows());
        assert!(data.flows.contains_key("react"));
    }
}
