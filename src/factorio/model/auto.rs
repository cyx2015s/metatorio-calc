use std::time::Instant;

use crate::{
    concept::EntryOpRequest,
    error::AppError,
    factorio::{DataContext, ProjectContext, planner::FactoryInstance, sort_generic_items_owned},
    math::flow_add,
};

/// 持有初始工厂实例和游戏上下文，返回一个新的工厂实例
/// 上下文我就不搞复杂的多线程共享了，
/// 1. 复制相对来讲是一次性的，可以接受
/// 2. 规划本身才是耗时的
pub fn factorio_auto_planner(
    mut factory: FactoryInstance,
    data: DataContext,
    proj: ProjectContext,
) -> Result<FactoryInstance, AppError> {
    log::info!("开始自动规划工厂实例: {}", factory.name);

    let instant = Instant::now();
    for mechanic in &mut factory.mechanics {
        mechanic.auto_populate(&data, &proj, &factory.factory);
        log::info!(
            "机制 {} 填充了 {} 个实例",
            mechanic.name(),
            mechanic.instance_len()
        );
    }
    log::info!("自动填充机制实例完成，用时: {:.2?}", instant.elapsed());
    let instant = Instant::now();
    factory.strict_source = true;
    let mut problem = factory.as_problem(&data, &proj);
    let solution = problem.solve();
    match solution {
        Ok(solution) => {
            factory.total_flow.clear();
            factory.solution = solution;
            for (idx, mechanic) in factory.mechanics.iter().enumerate() {
                for (jdx, instance) in mechanic.instances().iter().enumerate() {
                    let var_value = factory.solution.0.get(&(idx, jdx)).cloned().unwrap_or(0.0);
                    let flow = instance.as_flow(&data, &proj, &factory.factory);
                    factory.total_flow = flow_add(&factory.total_flow, &flow, var_value);
                }
            }
            sort_generic_items_owned(&mut factory.total_flow_sorted_keys, &data);
        }
        Err(e) => {
            log::error!("自动规划失败: {:?}", e);
            crate::toast::error(format!("自动规划失败。{:?}", e));
            return Err(AppError::Solver("无法获得结果".into()));
        }
    }

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
                            if *n < 1e-10 {
                                EntryOpRequest::Drop
                            } else {
                                EntryOpRequest::None
                            }
                        }
                        None => EntryOpRequest::Drop,
                    },
                );
            }
            mechanic.submit_operations();
        });

    factory.name += " (自动规划)";
    log::info!("自动规划完成: {}", factory.name);
    log::info!("自动规划用时: {:.2?}", instant.elapsed());
    Ok(factory)
}
