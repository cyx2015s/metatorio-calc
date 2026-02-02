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
    // 如果是严格模式，相比普通模式有如下限制：不出现在target中的物品必须配平，只能使用来自external的输入
    pub strict: bool,
}

pub type BasicSolverArgs<I, R> = (Flow<I>, IndexMap<R, (Flow<I>, f64)>);
pub type SolverArgs<I, R> = (Flow<I>, IndexMap<R, (Flow<I>, f64)>, Flow<I>);
pub type SolverSolution<R> = Result<(Flow<R>, f64), AppError>;

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
            strict: false,
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

    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    pub fn solve(&self) -> Result<(Flow<R>, f64), AppError> {
        let mut problem_variables = good_lp::ProblemVariables::new();
        let mut flow_vars = HashMap::new();
        let mut source_vars = HashMap::new();
        for recipe_id in self.flows.keys() {
            let var = problem_variables.add(variable().min(0));
            flow_vars.insert(recipe_id.clone(), var);
        }
        let mut item_balances = HashMap::new();

        for (recipe_id, flow) in &self.flows {
            let var = flow_vars.get(recipe_id).unwrap();
            for (item_id, &amount) in &flow.0 {
                let entry = item_balances
                    .entry(item_id.clone())
                    .or_insert(good_lp::Expression::from(0.0));
                *entry += amount * *var;
            }
        }
        for (item_id, _) in &self.sources {
            let var = problem_variables.add(variable().min(0));
            source_vars.insert(item_id.clone(), var);
            let entry = item_balances
                .entry(item_id.clone())
                .or_insert(good_lp::Expression::from(0.0));
            *entry += 1.0 * var;
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
            let balance = item_balances.get(item_id);
            if let Some(expr) = balance {
                targets.push(expr.clone().eq(amount));
            } else {
                if self.strict {
                    return Err(AppError::Solver(format!(
                        "物品 {:?} 没有相关配方，且处于严格模式，求解器无法继续。",
                        item_id
                    )));
                }
            }
        }
        let mut constraints = Vec::new();
        for (item_id, expr) in &item_balances {
            if !self.target.contains_key(item_id) && !no_providers.contains(item_id) {
                if self.strict {
                    constraints.push(expr.clone().eq(0.0));
                } else {
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
            optimization_expr += *cost * *var;
        }
        for (item_id, cost) in &self.sources {
            let var = source_vars.get(item_id).unwrap();
            optimization_expr += *cost * *var;
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
        solution_tx: std::sync::mpsc::Sender<SolverSolution<R>>,
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

/// 求解流程：从所有的 AsFlow 配方收集 Flow 信息
pub fn basic_solver<I, R>(
    target: Flow<I>,                    // 目标物品及其需求量
    flows: IndexMap<R, (Flow<I>, f64)>, // 配方标识符及其物品流和代价
) -> Result<(Flow<R>, f64), AppError>
where
    I: ItemIdent,
    R: ItemIdent,
{
    SolverData::new(target, flows).solve()
}
