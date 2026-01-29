use good_lp::{IntoAffineExpression, Solution, SolverModel, variable};
use indexmap::IndexMap;

use crate::concept::{Flow, ItemIdent};
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

#[derive(Debug, Clone)]
pub struct SolverData<I, R>
where
    I: ItemIdent,
    R: ItemIdent,
{
    target: Flow<I>,
    flows: IndexMap<R, (Flow<I>, f64)>,
    external: IndexMap<I, f64>, //  输入以下物品消耗的价值
}

pub type BasicSolverArgs<I, R> = (Flow<I>, IndexMap<R, (Flow<I>, f64)>);
pub type SolverSolution<R> = Result<(Flow<R>, f64), String>;

impl<I, R> SolverData<I, R>
where
    I: ItemIdent,
    R: ItemIdent,
{
    pub fn new(target: Flow<I>, flows: IndexMap<R, (Flow<I>, f64)>) -> Self {
        Self {
            target,
            flows,
            external: IndexMap::new(),
        }
    }

    pub fn with_external(mut self, external: IndexMap<I, f64>) -> Self {
        self.external.extend(external);
        self
    }

    pub fn solve(&self) -> Result<(Flow<R>, f64), String> {
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
        for (item_id, _) in &self.external {
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
        let mut targets = Vec::new();
        for (item_id, &amount) in &self.target {
            let balance = item_balances.get(item_id);
            if let Some(expr) = balance {
                targets.push(expr.clone().eq(amount));
            } else {
                return Err(format!("这个物品没有相关配方： {:?}", item_id));
            }
        }
        let mut constraints = Vec::new();
        for (item_id, expr) in &item_balances {
            if !self.target.contains_key(item_id) && !no_providers.contains(item_id) {
                constraints.push(expr.clone().geq(0.0));
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
        for (item_id, cost) in &self.external {
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
                    good_lp::ResolutionError::Str(s) => format!("求解过程中发生内部错误：{}", s),
                };
                if !no_providers.is_empty() {
                    let mut no_providers = no_providers.iter().collect::<Vec<_>>();
                    no_providers.sort_by_key(|x| format!("{:?}", x));
                    // err_string += format!("此外，以下物品缺少生产来源：{:?}", no_providers).as_str();
                }
                Err(err_string)
            }
        }
    }

    pub fn make_basic_solver_thread(
        solution_tx: std::sync::mpsc::Sender<SolverSolution<R>>,
        arg_rx: std::sync::mpsc::Receiver<BasicSolverArgs<I, R>>,
    ) {
        std::thread::spawn(move || {
            log::info!("求解线程启动");
            while let Ok((target, flows)) = arg_rx.recv() {
                let solver_data = SolverData::new(target, flows);
                // log::info!("收到了新的计算请求……");
                if solution_tx.send(solver_data.solve()).is_err() {
                    log::info!("求解线程退出");
                    // 接收方已关闭，退出线程
                    break;
                }
            }
        });
    }
}

/// 求解流程：从所有的 AsFlow 配方收集 Flow 信息
pub fn basic_solver<I, R>(
    target: Flow<I>,                    // 目标物品及其需求量
    flows: IndexMap<R, (Flow<I>, f64)>, // 配方标识符及其物品流和代价
) -> Result<(Flow<R>, f64), String>
where
    I: ItemIdent,
    R: ItemIdent,
{
    SolverData::new(target, flows).solve()
}
