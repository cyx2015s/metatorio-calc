//! 复现"小目标可解、大目标不可解"的宏观行为：
//! 构造带回收循环与大量冗余变量的链（模拟 auto_plan 大 LP），
//! 目标常数从 1e-6 到 10 扫描，观察可解性边界。
use metatorio_solver::{AIndexMap, SolverData, SolverSolution};

fn solve_chain(target_amount: f64, redundant: usize) -> SolverSolution<String, String> {
    let mut target = AIndexMap::default();
    target.insert("out".to_string(), target_amount);

    let mut flows = AIndexMap::default();
    // 主链：scrap → mid1 → mid2 → out
    let mut f1 = AIndexMap::default();
    f1.insert("scrap".to_string(), -1.0);
    f1.insert("mid1".to_string(), 1.0);
    flows.insert("step1".to_string(), (f1, 1.0));
    let mut f2 = AIndexMap::default();
    f2.insert("mid1".to_string(), -1.0);
    f2.insert("mid2".to_string(), 1.0);
    flows.insert("step2".to_string(), (f2, 1.0));
    let mut f3 = AIndexMap::default();
    f3.insert("mid2".to_string(), -1.0);
    f3.insert("out".to_string(), 1.0);
    flows.insert("step3".to_string(), (f3, 1.0));
    // 回收循环：out → mid1（0.5 比例），与生产形成环
    let mut recycle = AIndexMap::default();
    recycle.insert("out".to_string(), -0.5);
    recycle.insert("mid1".to_string(), 0.5);
    flows.insert("recycle".to_string(), (recycle, 1.0));
    // 冗余变量：大量"无用"配方（消耗/产出未使用物品，模拟大 LP 冗余）
    for i in 0..redundant {
        let mut waste = AIndexMap::default();
        waste.insert(format!("waste-in-{i}"), -1.0);
        waste.insert(format!("waste-out-{i}"), 1.0);
        flows.insert(format!("waste-{i}"), (waste, 1.0));
    }

    let mut sources = AIndexMap::default();
    sources.insert("scrap".to_string(), 1.0);

    let mut problem = SolverData::new_simple(target, flows);
    problem.sources = sources;
    problem.strict_source = true;
    problem.solve()
}

#[test]
fn large_target_fails_small_succeeds() {
    // 100 个冗余变量：模拟大 LP 的数值病态。
    let mut outcomes = Vec::new();
    for amount in [1e-6, 1e-4, 0.001, 0.01, 0.1, 1.0, 10.0] {
        let solution = solve_chain(amount, 100);
        let ok = matches!(solution, SolverSolution::Solved { .. });
        eprintln!("目标 {amount}: {}", if ok { "Solved" } else { "NotSolved" });
        outcomes.push((amount, ok));
    }
    // 断言：所有目标倍率都应可解（倍率不变性）。
    for (amount, ok) in &outcomes {
        assert!(*ok, "目标 {amount} 应可解（倍率不变性）");
    }
}
