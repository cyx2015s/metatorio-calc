use std::time::Instant;

use crate::{
    error::AppError,
    factorio::{DataContext, ProjectContext, planner::FactoryInstance, sort_generic_items_owned},
    math::SolverSolution,
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
    let start_time = Instant::now();
    let instant = Instant::now();
    for mechanic in &mut factory.mechanics {
        mechanic.auto_populate(&data, &proj, &factory.factory);
        log::debug!(
            "机制 {} 填充了 {} 个实例",
            mechanic.name(),
            mechanic.instance_len()
        );
    }
    log::info!("自动填充机制实例完成，用时: {:.2?}", instant.elapsed());
    let instant = Instant::now();

    factory.strict_source = true;

    let mut problem = factory.as_problem(&data, &proj);

    log::info!(
        "构建求解器问题完成，变量数量: {}, 用时: {:.2?}",
        problem.flows.len(),
        instant.elapsed()
    );

    let instant = Instant::now();
    let solution = problem.solve();
    match solution {
        SolverSolution::Solved { ref sum, .. } => {
            factory.total_flow_sorted_keys = sum.keys().cloned().collect();

            sort_generic_items_owned(&mut factory.total_flow_sorted_keys, &data);
        }
        SolverSolution::NotSolved {
            no_provider,
            no_consumer,
            description,
        } => {
            log::error!(
                "自动规划失败: no_provider={:?}..., no_consumer={:?}...",
                &no_provider,
                &no_consumer
            );
            crate::toast::error(format!("自动规划失败。{}", description));
            return Err(AppError::Solver("无法获得结果".into()));
        }
    }

    factory.solution = solution;

    factory.trim_flows();

    factory.name += " (自动规划)";
    let end_time = Instant::now();
    log::info!("自动规划完成: {}", factory.name);
    log::info!("线性规划用时: {:.2?}", instant.elapsed());
    log::info!(
        "自动规划总用时: {:.2?}",
        end_time.duration_since(start_time)
    );
    Ok(factory)
}
