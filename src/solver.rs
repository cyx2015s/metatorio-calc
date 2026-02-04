use good_lp::{IntoAffineExpression, Solution, SolverModel, variable};
use indexmap::IndexMap;

use crate::concept::{Flow, ItemIdent};
use crate::error::AppError;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;

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

/// 返回值仅用作 AsFlowEditor 的唯一标识符
#[allow(clippy::borrowed_box)]
pub fn box_as_ptr<T: ?Sized>(b: &Box<T>) -> usize {
    &**b as *const T as *const () as usize
}

pub fn ref_as_ptr<T: ?Sized>(r: &T) -> usize {
    r as *const T as *const () as usize
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

pub enum SolverSolution<I, R> {
    Solved {
        prim: Flow<R>,
        dual: Option<Flow<I>>,
    },
    NotSolved {
        no_provider: Vec<I>,
        no_consumer: Vec<I>,
    },
}

pub type SolverSolutionTuple<R> = Result<(Flow<R>, f64), AppError>;

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

    pub fn solve(&self) -> Result<(Flow<R>, f64), AppError> {
        // 先做一步数值稳定性处理，将出现的流系数全部统一到1附近。
        let mut magnitude_and_counts = HashMap::new();
        log::info!("求解器：开始分析流量数量级");
        for recipe in self.flows.values() {
            for (item_id, &amount) in &recipe.0 {
                let entry = magnitude_and_counts
                    .entry(item_id.clone())
                    .or_insert((0.0, 0));
                entry.0 += amount.abs().log2().max(-32.0);
                entry.1 += 1;
            }
        }
        // total_magnitude / count 是这些数据的几何平均数的数量级
        let magnitude_factors: HashMap<I, i32> = magnitude_and_counts
            .into_iter()
            .map(|(item, (total_maginitude, count))| {
                (item, (-total_maginitude / count as f64) as i32)
            })
            .collect();
        let get_multiplier =
            |item_id: &I| -> f64 { (2.0_f64).powi(*magnitude_factors.get(item_id).unwrap_or(&0)) };
        let mut problem_variables = good_lp::ProblemVariables::new();
        let mut flow_vars = IndexMap::new();
        let mut source_vars = IndexMap::new();
        let mut sink_vars = IndexMap::new();
        for recipe_id in self.flows.keys() {
            let var = problem_variables.add(variable().min(0));
            flow_vars.insert(recipe_id.clone(), var);
        }
        let mut item_balances = IndexMap::new();
        log::info!(
            "求解器：开始构建物品平衡表达式：一共有 {} 个配方变量",
            self.flows.len()
        );
        for (recipe_id, flow) in &self.flows {
            let var = flow_vars.get(recipe_id).unwrap();
            for (item_id, &amount) in &flow.0 {
                let entry = item_balances
                    .entry(item_id.clone())
                    .or_insert(good_lp::Expression::from(0.0));
                *entry += amount * get_multiplier(item_id) * *var;
            }
        }
        log::info!("求解器：一共有 {} 个物品需要平衡", item_balances.len());
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
        let mut no_providers: HashSet<I> = item_balances.keys().cloned().collect();
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
        let mut targets = Vec::new();
        for (item_id, &amount) in &self.target {
            // 目标物品，严格相等
            let balance = item_balances.get(item_id);
            if let Some(expr) = balance {
                targets.push(expr.clone().eq(amount * get_multiplier(item_id)));
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
                } else {
                    if no_providers.contains(item_id) {
                    } else {
                        if self.strict_sink {
                            // 必须配平
                            constraints.push(expr.clone().eq(0.0));
                        } else {
                            // 不用配平
                            constraints.push(expr.clone().geq(0.0));
                        }
                    }
                }
            }
        }
        for source_var in source_vars.values() {
            constraints.push(source_var.into_expression().geq(0.0));
        }
        let mut optimization_expr = good_lp::Expression::from(0.0);
        for (flow, (_, cost)) in &self.flows {
            let var = flow_vars.get(flow).unwrap();
            optimization_expr += *cost * *var;
        }
        for (item_id, cost) in &self.sources {
            let var = source_vars.get(item_id).unwrap();
            optimization_expr += *cost / get_multiplier(item_id) * *var;
        }
        for (item_id, cost) in &self.sinks {
            let var = sink_vars.get(item_id).unwrap();
            optimization_expr += *cost / get_multiplier(item_id) * *var;
        }
        if no_providers.len() > 0 {
            log::warn!("没有来源的物品：{:?}", &no_providers);
        }
        let solution = problem_variables
            .minimise(&optimization_expr)
            .using(good_lp::default_solver)
            .with_all(targets)
            .with_all(constraints)
            .solve();

        match solution {
            Ok(sol) => {
                let mut result = IndexMap::new();
                for (recipe_id, var) in flow_vars {
                    let value = sol.value(var);
                    result.insert(recipe_id.clone(), value);
                }
                Ok((result, sol.eval(&optimization_expr)))
            }
            Err(err) => {
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
                if !no_providers.is_empty() {
                    let mut no_providers = no_providers.iter().collect::<Vec<_>>();
                    no_providers.sort_by_key(|x| format!("{:?}", x));
                    // err_string += format!("此外，以下物品缺少生产来源：{:?}", no_providers).as_str();
                }
                Err(AppError::Solver(err_string))
            }
        }
    }

    pub fn make_solver_thread(
        solution_tx: std::sync::mpsc::Sender<SolverSolutionTuple<R>>,
        arg_rx: std::sync::mpsc::Receiver<SolverData<I, R>>,
    ) {
        std::thread::spawn(move || {
            log::info!("求解线程启动");
            loop {
                let mut last_req = match arg_rx.recv() {
                    Ok(req) => req,
                    Err(_) => break,
                };
                // 尽可能多地丢弃后续请求，只保留最新
                while let Ok(req) = arg_rx.try_recv() {
                    // 虽然不太可能，因为每次算都很快。
                    log::info!("丢弃了一个过时的求解请求");

                    last_req = req;
                }

                // log::info!("收到了新的计算请求……");
                if solution_tx.send(last_req.solve()).is_err() {
                    // 接收方已关闭，退出线程
                    break;
                }
            }
            log::info!("求解线程退出");
        });
    }
}
