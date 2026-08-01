//! 线程求解器集成测试：模拟主 crate `ProjectInstance` 的用法
//! （`problem_sender` / `solution_receiver` channel 模式）。

use std::sync::mpsc::channel;
use std::time::Duration;

use metatorio_solver::{AIndexMap, SolverData, SolverSolution};

fn smelting_problem(target_amount: f64) -> SolverData<&'static str, &'static str> {
    let mut target = AIndexMap::default();
    target.insert("iron-plate", target_amount);

    let mut flows = AIndexMap::default();
    let mut smelt = AIndexMap::default();
    smelt.insert("iron-ore", -1.0);
    smelt.insert("iron-plate", 1.0);
    flows.insert("smelt", (smelt, 1.0));

    SolverData::new_simple(target, flows)
}

fn assert_solved(context: &str, solution: SolverSolution<&'static str, &'static str>) {
    match solution {
        SolverSolution::Solved { .. } => {}
        SolverSolution::NotSolved { description, .. } => {
            panic!("{context}: 求解失败: {description}")
        }
    }
}

#[test]
fn dedicated_solver_thread_solves_and_returns() {
    let (solution_tx, solution_rx) = channel();
    let (problem_tx, problem_rx) = channel();
    SolverData::make_dedicated_solver_thread(solution_tx, problem_rx);

    problem_tx.send(smelting_problem(1.0)).unwrap();
    let solution = solution_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("10s 内未收到求解结果");

    match solution {
        SolverSolution::Solved { prim, sum, cost, .. } => {
            assert!(
                (prim["smelt"] - 1.0).abs() < 1e-5,
                "prim: {prim:?}"
            );
            assert!((sum["iron-plate"] - 1.0).abs() < 1e-5, "sum: {sum:?}");
            assert!((cost - 1.0).abs() < 1e-5, "cost: {cost}");
        }
        SolverSolution::NotSolved { description, .. } => panic!("求解失败: {description}"),
    }

    drop(problem_tx); // 关闭输入通道，线程在下次 recv 时退出
}

#[test]
fn dedicated_solver_thread_handles_sequential_requests() {
    // 连续两次请求（串行，等前一次结果返回后再发下一次）：
    // 线程应持续存活并各自返回正确结果。
    //
    // 注：线程的"丢弃过期请求"逻辑（try_recv 后只保留最新）依赖发送时序，
    // 是尽力而为的优化而非可测试的契约，故此处不验证。
    let (solution_tx, solution_rx) = channel();
    let (problem_tx, problem_rx) = channel();
    SolverData::make_dedicated_solver_thread(solution_tx, problem_rx);

    problem_tx.send(smelting_problem(1.0)).unwrap();
    let r1 = solution_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("10s 内未收到第一个结果");
    match r1 {
        SolverSolution::Solved { sum, .. } => {
            assert!((sum["iron-plate"] - 1.0).abs() < 1e-5, "{sum:?}");
        }
        SolverSolution::NotSolved { description, .. } => panic!("第一次求解失败: {description}"),
    }

    problem_tx.send(smelting_problem(2.0)).unwrap();
    let r2 = solution_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("10s 内未收到第二个结果");
    match r2 {
        SolverSolution::Solved { sum, .. } => {
            assert!((sum["iron-plate"] - 2.0).abs() < 1e-5, "{sum:?}");
        }
        SolverSolution::NotSolved { description, .. } => panic!("第二次求解失败: {description}"),
    }
    drop(problem_tx);
}

#[test]
fn batched_solver_thread_aggregates_multiple_requests() {
    // make_solver_thread：50ms 批处理窗口内合并多个请求，按 (id, result) 返回
    let (solution_tx, solution_rx) = channel();
    let (problem_tx, problem_rx) = channel();
    SolverData::make_solver_thread(solution_tx, problem_rx);

    problem_tx.send((0usize, smelting_problem(1.0))).unwrap();
    problem_tx.send((1usize, smelting_problem(3.0))).unwrap();

    let mut results = Vec::new();
    for _ in 0..2 {
        let (id, solution) = solution_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("10s 内未收到求解结果");
        assert_solved("批处理线程", solution.clone());
        let sum_plate = match &solution {
            SolverSolution::Solved { sum, .. } => sum["iron-plate"],
            _ => unreachable!(),
        };
        results.push((id, sum_plate));
    }
    results.sort_by_key(|(id, _)| *id);

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, 0);
    assert!((results[0].1 - 1.0).abs() < 1e-5, "{results:?}");
    assert_eq!(results[1].0, 1);
    assert!((results[1].1 - 3.0).abs() < 1e-5, "{results:?}");
    drop(problem_tx);
}
