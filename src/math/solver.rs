use good_lp::{IntoAffineExpression, Solution, SolverModel, variable};
use indexmap::IndexMap;

use crate::concept::{Flow, ItemIdent};
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::mpsc::*;

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

#[derive(Debug, Clone)]
pub struct SolverData<I, R>
where
    I: ItemIdent,
    R: ItemIdent,
{
    pub target: Flow<I>,
    pub flows: IndexMap<R, (Flow<I>, f64)>,
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
pub enum SolverSolution<I, R> {
    Solved {
        prim: Flow<R>,
        dual: Option<Flow<I>>,
        prim_scale: Flow<R>,
        dual_scale: Flow<I>,
        sum: Flow<I>,
        target_scale: f64,
        cost: f64,
    },
    NotSolved {
        no_provider: Vec<I>,
        no_consumer: Vec<I>,
        description: String,
    },
}

impl<I, R> SolverSolution<I, R>
where
    I: ItemIdent,
    R: ItemIdent,
{
    pub fn get_prim_of(&self, i: &R) -> Option<f64> {
        match self {
            SolverSolution::Solved {
                prim,
                prim_scale,
                target_scale,
                ..
            } => match (prim.get(i), prim_scale.get(i)) {
                (Some(v), Some(s)) => Some(*v * s / target_scale),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn get_prim_raw(&self) -> Option<&Flow<R>> {
        match self {
            SolverSolution::Solved { prim, .. } => Some(prim),
            _ => None,
        }
    }

    pub fn get_prim_raw_of(&self, i: &R) -> Option<f64> {
        match self {
            SolverSolution::Solved { prim, .. } => prim.get(i).cloned(),
            _ => None,
        }
    }

    pub fn get_dual_of(&self, i: &I) -> Option<f64> {
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

    pub fn get_dual_raw(&self) -> Option<&Flow<I>> {
        match self {
            SolverSolution::Solved {
                dual: Some(dual), ..
            } => Some(dual),
            _ => None,
        }
    }

    pub fn get_dual_raw_of(&self, i: &I) -> Option<f64> {
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
                sum, dual_scale, ..
            } => sum
                .get(i)
                .map(|v| *v * dual_scale.get(i).cloned().unwrap_or(1.0)),
            _ => None,
        }
    }
}

impl<I, R> SolverData<I, R>
where
    I: ItemIdent,
    R: ItemIdent,
{
    pub fn new(target: Flow<I>, flows: IndexMap<R, (Flow<I>, f64)>) -> Self {
        Self {
            target,
            flows,
            sources: IndexMap::new(),
            sinks: IndexMap::new(),
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
            let mut no_providers: HashSet<I> = HashSet::new();

            for flow in self.flows.values() {
                for (item_id, _) in &flow.0 {
                    no_providers.insert(item_id.clone());
                }
            }

            for flow in self.flows.values() {
                for (item_id, &amount) in &flow.0 {
                    if amount > 0.0 {
                        no_providers.remove(item_id);
                    }
                }
            }

            for item in self.sources.keys() {
                no_providers.remove(item);
            }
            let before = self.flows.len();

            self.flows.retain(|_, v: &mut (IndexMap<I, f64>, f64)| {
                if v.0
                    .iter()
                    .any(|(item_id, &amount)| no_providers.contains(item_id) && amount < 0.0)
                {
                    // 有任一物品出现在了这个流中，并且是消耗的，说明这个流无法使用
                    changed = true;
                    false
                } else {
                    true
                }
            });
            let after = self.flows.len();
            if before != after {
                log::info!(
                    "求解器：移除了 {} 个无法使用的配方 ({} -> {})",
                    before - after,
                    before,
                    after
                );
            }
            log::info!(
                "求解器：移除无法使用的配方耗时 {} ms",
                instant.elapsed().as_millis()
            );
        }
        changed
    }

    pub fn solve(&mut self) -> SolverSolution<I, R> {
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
        while self.trim_flows() {}
        // 调整配方的系数，使得其中出现的数量级最大的物品的数量级在1附近，避免数值不稳定。
        let mut item_scales: HashMap<I, f64> = HashMap::new();
        for flow in self.flows.values() {
            for (item_id, _) in &flow.0 {
                item_scales.insert(item_id.clone(), 0.0);
            }
        }
        for (item_id, _) in &self.target {
            item_scales.insert(item_id.clone(), 0.0);
        }
        let mut flow_scales: HashMap<R, f64> =
            self.flows.keys().map(|id| (id.clone(), 0.0)).collect();
        let mut target_scale: f64 = 0.0;
        log::info!("开始平衡数量级");
        for i in 0..4 {
            // 1. 计算 flow_scales (列缩放)
            for (f_id, flow) in &self.flows {
                let mut sum_log = 0.0;
                let mut count = 0;
                for (i_id, amount) in &flow.0 {
                    if amount.abs() > 1e-12 {
                        // 当前值 = 原始值 * 物品缩放
                        let current_val =
                            amount.abs() * 2.0_f64.powf(*item_scales.get(i_id).unwrap());
                        sum_log += current_val.log2();
                        count += 1;
                    }
                }
                if count > 0 {
                    // 目标是让 sum_log + log2(flow_multiplier) 趋近于 0
                    flow_scales.insert(f_id.clone(), -(sum_log / count as f64));
                }
            }
            let mut target_sum_log = 0.0;
            let mut target_count = 0;
            for (i_id, &amount) in &self.target {
                if amount.abs() > 1e-12 {
                    let current_val = amount.abs() * 2.0_f64.powf(*item_scales.get(i_id).unwrap());
                    target_sum_log += current_val.log2();
                    target_count += 1;
                }
                if target_count > 0 {
                    target_scale = -(target_sum_log / target_count as f64);
                }
            }

            // 2. 计算 item_scales (行缩放)
            let mut item_stats: HashMap<I, (f64, usize)> = HashMap::new();
            for (f_id, flow) in &self.flows {
                let f_scale = *flow_scales.get(f_id).unwrap();
                for (i_id, amount) in &flow.0 {
                    if amount.abs() > 1e-12 {
                        let current_val = amount.abs() * 2.0_f64.powf(f_scale);
                        let entry = item_stats.entry(i_id.clone()).or_insert((0.0, 0));
                        entry.0 += current_val.log2();
                        entry.1 += 1;
                    }
                }
            }
            for (i_id, target) in &self.target {
                if target.abs() > 1e-12 {
                    let current_val = target.abs() * 2.0_f64.powf(target_scale);
                    let entry = item_stats.entry(i_id.clone()).or_insert((0.0, 0));
                    entry.0 += current_val.log2();
                    entry.1 += 1;
                }
            }
            for (i_id, (sum_log, count)) in item_stats {
                item_scales.insert(i_id, -(sum_log / count as f64));
            }
            log::info!("第{i}轮数量级平衡完成。",);
            log::info!("target = {:?}, target_scale = {target_scale}", &self.target);
        }
        log::info!("数量级平衡完成",);
        // 应用

        let get_item_scale =
            |item_id: &I| -> f64 { (2.0_f64).powf(*item_scales.get(item_id).unwrap_or(&0.0)) };
        let get_flow_scale =
            |flow_id: &R| -> f64 { (2.0_f64).powf(*flow_scales.get(flow_id).unwrap_or(&0.0)) };
        let target_scale = (2.0_f64).powf(target_scale);
        let mut problem_variables = good_lp::ProblemVariables::new();
        let mut flow_vars = IndexMap::new();
        let mut source_vars = IndexMap::new();
        let mut sink_vars = IndexMap::new();
        for f_id in self.flows.keys() {
            let var = problem_variables.add(variable().min(0));
            flow_vars.insert(f_id.clone(), var);
        }
        let mut item_balances = IndexMap::new();
        log::info!(
            "求解器：开始构建物品平衡表达式：一共有 {} 个配方变量",
            self.flows.len()
        );
        let mut extreme = 0.0;
        for (f_id, flow) in &self.flows {
            let var = flow_vars.get(f_id).unwrap();
            for (item_id, &amount) in &flow.0 {
                let entry = item_balances
                    .entry(item_id.clone())
                    .or_insert(good_lp::Expression::from(0.0));
                let val = amount * get_item_scale(item_id) * get_flow_scale(f_id);
                if val.abs().log2().abs() > extreme {
                    extreme = val.abs().log2();
                }
                *entry += val * *var;
            }
        }
        log::info!(
            "求解器：一共有 {} 个物品需要平衡，矩阵元素的最大数量级为 {:.2}",
            item_balances.len(),
            extreme
        );
        for (item_id, _) in &self.sources {
            let var = problem_variables.add(variable().min(0));
            source_vars.insert(item_id.clone(), var);
            let entry = item_balances
                .entry(item_id.clone())
                .or_insert(good_lp::Expression::from(0.0));
            *entry += 1.0 * var * get_item_scale(item_id);
        }
        for (item_id, _) in &self.sinks {
            let var = problem_variables.add(variable().min(0));
            sink_vars.insert(item_id.clone(), var);
            let entry = item_balances
                .entry(item_id.clone())
                .or_insert(good_lp::Expression::from(0.0));
            *entry -= 1.0 * var * get_item_scale(item_id);
        }
        let mut no_providers: HashSet<I> = item_balances.keys().cloned().collect();
        let mut no_consumers: HashSet<I> = item_balances.keys().cloned().collect();
        for flow in self.flows.values() {
            for (item_id, &amount) in &flow.0 {
                if amount > 0.0 {
                    no_providers.remove(item_id);
                }
                if amount < 0.0 {
                    no_consumers.remove(item_id);
                }
            }
        }
        for item in self.sources.keys() {
            no_providers.remove(item);
        }
        for item in self.sinks.keys() {
            no_consumers.remove(item);
        }
        let mut targets = Vec::new();
        for (item_id, &amount) in &self.target {
            // 目标物品，严格相等
            let balance = item_balances.get(item_id);
            if let Some(expr) = balance {
                targets.push(
                    expr.clone()
                        .eq(amount * get_item_scale(item_id) * target_scale),
                );
            }
        }
        let mut constraints = Vec::new();
        for (item_id, expr) in &item_balances {
            if !self.target.contains_key(item_id) {
                // 严格模式下，不能凭空输入。非严格模式下，有来源的物品不能有凭空输入。
                // 非目标物品，不能为负
                if self.strict_source {
                    // 不能从外部借用
                    if self.strict_sink {
                        // 必须配平
                        constraints.push(expr.clone().eq(0.0));
                    } else {
                        // 不用配平
                        constraints.push(expr.clone().geq(0.0));
                    }
                } else if no_providers.contains(item_id) {
                } else if self.strict_sink {
                    // 必须配平
                    constraints.push(expr.clone().eq(0.0));
                } else {
                    // 不用配平
                    constraints.push(expr.clone().geq(0.0));
                }
            }
        }
        for source_var in source_vars.values() {
            constraints.push(source_var.into_expression().geq(0.0));
        }
        let mut optimization_expr = good_lp::Expression::from(0.0);
        for (flow, (_, cost)) in &self.flows {
            let var = flow_vars.get(flow).unwrap();
            optimization_expr += *cost * *var * get_flow_scale(flow);
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
            log::warn!("没有来源的物品：{:?}个", &no_providers.len());
        }
        if !no_consumers.is_empty() {
            log::warn!("没有去处的物品：{:?}个", &no_consumers.len());
        }
        let solution = problem_variables
            .minimise(&optimization_expr)
            .using(good_lp::microlp)
            .with_all(targets)
            .with_all(constraints)
            .solve();

        match solution {
            Ok(sol) => {
                log::info!("求解器：求解成功，开始构建结果");
                let mut sum = Flow::new();
                let mut prim = Flow::new();
                let mut prim_scale = Flow::new();
                for (f_id, var) in &flow_vars {
                    let value = sol.value(*var);
                    prim.insert(f_id.clone(), value);

                    prim_scale.insert(f_id.clone(), get_flow_scale(f_id));
                    for (item_id, &amount) in &self.flows[f_id].0 {
                        let entry = sum.entry(item_id.clone()).or_insert(0.0);
                        *entry += amount * value / target_scale * get_flow_scale(f_id);
                    }
                }
                SolverSolution::Solved {
                    prim,
                    prim_scale,
                    dual: None,
                    dual_scale: sum
                        .iter()
                        .map(|(i_id, _)| (i_id.clone(), get_item_scale(i_id)))
                        .collect(),
                    sum,
                    target_scale,
                    cost: sol.eval(optimization_expr) / target_scale,
                }
            }
            Err(err) => {
                log::error!("求解器：求解失败，错误信息: {:?}", err);
                let err_string = match err {
                    good_lp::ResolutionError::Unbounded => {
                        "无界。存在能够无限产生目标物品且不增加消耗的配方组合。".to_string()
                    }
                    good_lp::ResolutionError::Infeasible => {
                        "无解。不存在能够满足目标物品需求的配方组合。".to_string()
                    }
                    good_lp::ResolutionError::Other(_) => "求解过程中发生未知错误。".to_string(),
                    good_lp::ResolutionError::Str(s) => {
                        format!("求解过程中发生内部错误：{}", s)
                    }
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
            while let Ok((req_id, mut req)) = problem_rx.recv() {
                let result = req.solve();
                if solution_tx.send((req_id, result)).is_err() {
                    // 接收方已关闭，退出线程
                    break;
                }
            }
            log::info!("求解线程退出");
        });
    }
}
