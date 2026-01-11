use good_lp::{Solution, SolverModel, variable};

use crate::concept::ItemIdent;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;

pub fn basic_solver<I, R>(
    target: HashMap<I, f64>,                   // 目标物品及其需求量
    flows: HashMap<R, (HashMap<I, f64>, f64)>, // 配方标识符及其物品流
) -> Result<HashMap<R, f64>, String>
where
    I: ItemIdent,
    R: Eq + Hash + Clone + Debug,
{
    let mut problem_variables = good_lp::ProblemVariables::new();
    let mut flow_vars = HashMap::new();
    for recipe_id in flows.keys() {
        let var = problem_variables.add(variable().min(0));
        flow_vars.insert(recipe_id.clone(), var);
    }
    let mut item_balances = HashMap::new();

    for (recipe_id, flow) in &flows {
        let var = flow_vars.get(recipe_id).unwrap();
        for (item_id, &amount) in &flow.0 {
            let entry = item_balances
                .entry(item_id.clone())
                .or_insert(good_lp::Expression::from(0.0));
            *entry += amount * *var;
        }
    }
    let mut no_providers: HashSet<I> = item_balances.keys().cloned().collect();
    for flow in flows.values() {
        for (item_id, &amount) in &flow.0 {
            if amount > 0.0 {
                no_providers.remove(item_id);
            }
        }
    }
    let mut targets = Vec::new();
    for (item_id, &amount) in &target {
        let balance = item_balances.get(item_id);
        if let Some(expr) = balance {
            targets.push(expr.clone().eq(amount));
        } else {
            return Err(format!("这个物品没有相关配方： {:?}", item_id));
        }
    }
    let mut constraints = Vec::new();
    for (item_id, expr) in item_balances {
        if !target.contains_key(&item_id) {
            constraints.push(expr.geq(0.0));
        }
    }
    let mut optimization_expr = good_lp::Expression::from(0.0);
    for (flow, (_, cost)) in flows {
        let var = flow_vars.get(&flow).unwrap();
        optimization_expr += cost * *var;
    }
    let solution = problem_variables
        .minimise(&optimization_expr)
        .using(good_lp::default_solver)
        .with_all(targets)
        .with_all(constraints)
        .solve();
    match solution {
        Ok(sol) => {
            let mut result = HashMap::new();
            for (recipe_id, var) in flow_vars {
                let value = sol.value(var);
                result.insert(recipe_id.clone(), value);
            }
            log::debug!("求解完成，代价函数结果为: {}", sol.eval(&optimization_expr));
            Ok(result)
        }
        Err(err) => {
            let mut err_string = match err {
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
                err_string += format!("此外，以下物品缺少生产来源：{:?}", no_providers).as_str();
            }
            Err(err_string)
        }
    }
}
