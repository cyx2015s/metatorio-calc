use std::time::Instant;

use crate::{
    concept::EntryOperation,
    error::AppError,
    factorio::{FactorioContext, planner::FactoryInstance, sort_generic_items_owned},
    solver::flow_add,
};

/// 持有初始工厂实例和游戏上下文，返回一个新的工厂实例
/// 上下文我就不搞复杂的多线程共享了，
/// 1. 复制相对来讲是一次性的，可以接受
/// 2. 规划本身才是耗时的
pub fn factorio_auto_planner(
    mut factory: FactoryInstance,
    ctx: FactorioContext,
) -> Result<FactoryInstance, AppError> {
    log::info!("开始自动规划工厂实例: {}", factory.name);
    let instant = Instant::now();
    for mechanic in &mut factory.mechanics {
        mechanic.auto_populate(&ctx);
    }
    factory.send_solve_request(&ctx);
    match factory.solution_receiver.recv() {
        Ok(solution) => {
            factory.total_flow.clear();
            factory.solution = solution?;
            for (idx, mechanic) in factory.mechanics.iter().enumerate() {
                for (jdx, instance) in mechanic.instances().iter().enumerate() {
                    let var_value = factory.solution.0.get(&(idx, jdx)).cloned().unwrap_or(0.0);
                    let flow = instance.as_flow(&ctx);
                    factory.total_flow = flow_add(&factory.total_flow, &flow, var_value);
                }
            }
            // Update sorted keys cache when total_flow changes
            factory.total_flow_sorted_keys = factory.total_flow.keys().cloned().collect();
            sort_generic_items_owned(&mut factory.total_flow_sorted_keys, &ctx);
        }
        Err(e) => {
            log::error!("自动规划失败: {}", e);
            return Err(AppError::Solver("无法从求解线程获得结果。".into()));
        }
    }

    factory.total_flow.iter().for_each(|(item, amount)| {
        if *amount < 0.0 {
            // 虽然没有指定为外部输入，但实际上是外部输入
            // 目前常见于因为游戏机制复刻不全导致没法实际生产的物品
            // 燃料、电力、热量等等……
            if !factory.external.iter().any(|(i, _)| i == item) {
                factory.external.push((item.clone(), -*amount));
            }
        }
    });

    factory.strict_source = true;

    factory
        .mechanics
        .iter_mut()
        .enumerate()
        .for_each(|(idx, mechanic)| {
            for jdx in 0..mechanic.instance_len() {
                mechanic.instance_operate(
                    jdx,
                    &mut |_| match factory.solution.0.get(&(idx, jdx)) {
                        Some(n) => {
                            if n.abs() < 1e-12 {
                                EntryOperation::Drop
                            } else {
                                EntryOperation::None
                            }
                        }
                        None => EntryOperation::Drop,
                    },
                );
            }
        });
    factory.name = factory.name + " (自动规划)";
    log::info!("自动规划完成: {}", factory.name);
    log::info!("自动规划用时: {:.2?}", instant.elapsed());
    Ok(factory)
}
