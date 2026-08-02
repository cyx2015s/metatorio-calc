//! 大规模配方链集成测试：验证求解器在真实规模问题下的正确性。
//!
//! ## 循环配方网络的行为说明（2026-08-01 拆分测试时确认）
//!
//! 循环网络（如 lossy: 2a→1b 与 recycle: 1b→1a）在**没有外部输入**时
//! 对 target 净产出 b 不可行是**正确的数学行为**：balance_b = L - R = 1
//! 要求 R = L - 1，而 balance_a = -2L + R = -L - 1 < 0 恒成立
//! （损耗循环必然净消耗 a），a 无外部来源 → Infeasible。
//! 添加外部补给（sources）后可解，且最优解可由手算验证。
//!
//! 求解器本身无数值问题（`src/ruiz.rs` 的 `test_ruiz` 已验证最优性）。
//! 曾误判为数值缺陷的两个案例在此分别以"预期不可行"与
//! "外部补给可解"的契约测试形式固化。
use metatorio_solver::{AIndexMap, SolverData, SolverSolution};

/// 构造 n 级配方链：item-0 → item-1 → ... → item-n，各级成本递增。
fn chain_problem(stages: usize, target_amount: f64) -> SolverData<String, String> {
    let mut flows = AIndexMap::default();
    for i in 0..stages {
        let mut coeffs = AIndexMap::default();
        coeffs.insert(format!("item-{i}"), -1.0);
        coeffs.insert(format!("item-{}", i + 1), 1.0);
        flows.insert(format!("step-{i}"), (coeffs, 1.0 + i as f64 * 0.1));
    }
    let mut target = AIndexMap::default();
    target.insert(format!("item-{stages}"), target_amount);
    SolverData::new_simple(target, flows)
}

#[test]
fn solve_20_stage_chain_propagates_amount() {
    let solution = chain_problem(20, 3.0).solve();
    match solution {
        SolverSolution::Solved {
            prim, sum, cost, ..
        } => {
            // 每级配方都必须运行 3 次（逐级传递目标量）
            for i in 0..20 {
                let key = format!("step-{i}");
                assert!(
                    (prim[&key] - 3.0).abs() < 1e-4,
                    "step-{i} 应为 3.0: {prim:?}"
                );
            }
            // 起点消耗 3，终点产出 3，中间物配平
            assert!((sum["item-0"] + 3.0).abs() < 1e-4, "sum: {sum:?}");
            for i in 1..20 {
                let key = format!("item-{i}");
                assert!((sum[&key]).abs() < 1e-4, "中间物 item-{i} 应配平: {sum:?}");
            }
            assert!((sum["item-20"] - 3.0).abs() < 1e-4, "sum: {sum:?}");
            // 成本 = Σ (1 + 0.1*i) * 3
            let expected_cost: f64 = (0..20).map(|i| (1.0 + i as f64 * 0.1) * 3.0).sum();
            assert!(
                (cost - expected_cost).abs() < 1e-3,
                "cost: {cost} vs {expected_cost}"
            );
        }
        SolverSolution::NotSolved { description, .. } => panic!("求解失败: {description}"),
    }
}

#[test]
fn solve_50_stage_chain_does_not_timeout() {
    // 冒烟：50 级链应快速求解（线性规划规模冒烟）
    let solution = chain_problem(50, 1.0).solve();
    match solution {
        SolverSolution::Solved { sum, .. } => {
            assert!((sum["item-50"] - 1.0).abs() < 1e-4, "sum: {sum:?}");
        }
        SolverSolution::NotSolved { description, .. } => panic!("求解失败: {description}"),
    }
}

#[test]
fn solve_branching_network_with_shared_input() {
    // 分支网络：两个成品共享同一个中间物
    //   ore → plate（冶炼）
    //   plate → gear（加工，消耗 2 plate）
    //   plate → iron-stick（加工，消耗 1 plate）
    // 目标：gear 1 + iron-stick 1
    let mut flows = AIndexMap::default();

    let mut smelt = AIndexMap::default();
    smelt.insert("ore", -1.0);
    smelt.insert("plate", 1.0);
    flows.insert("smelt", (smelt, 1.0));

    let mut make_gear = AIndexMap::default();
    make_gear.insert("plate", -2.0);
    make_gear.insert("gear", 1.0);
    flows.insert("make-gear", (make_gear, 2.0));

    let mut make_stick = AIndexMap::default();
    make_stick.insert("plate", -1.0);
    make_stick.insert("iron-stick", 1.0);
    flows.insert("make-stick", (make_stick, 1.0));

    let mut target = AIndexMap::default();
    target.insert("gear", 1.0);
    target.insert("iron-stick", 1.0);

    let solution = SolverData::new_simple(target, flows).solve();
    match solution {
        SolverSolution::Solved {
            prim, sum, cost, ..
        } => {
            assert!((prim["make-gear"] - 1.0).abs() < 1e-4, "prim: {prim:?}");
            assert!((prim["make-stick"] - 1.0).abs() < 1e-4, "prim: {prim:?}");
            assert!((prim["smelt"] - 3.0).abs() < 1e-4, "共需 3 plate: {prim:?}");
            assert!((sum["ore"] + 3.0).abs() < 1e-4, "sum: {sum:?}");
            assert!((sum["plate"]).abs() < 1e-4, "plate 配平: {sum:?}");
            // 成本 = 3*smelt(1.0) + 1*gear(2.0) + 1*stick(1.0) = 6
            assert!((cost - 6.0).abs() < 1e-3, "cost: {cost}");
        }
        SolverSolution::NotSolved { description, .. } => panic!("求解失败: {description}"),
    }
}

#[test]
fn solve_recycling_loop_without_external_input_is_infeasible() {
    // 损耗回收循环（2a→b + b→a），无外部输入，target b 净产出 1：
    // 数学上必然净消耗 a（balance_a = -L - 1 < 0），无解是预期行为。
    let mut flows = AIndexMap::default();
    let mut lossy = AIndexMap::default();
    lossy.insert("a", -2.0);
    lossy.insert("b", 1.0);
    flows.insert("lossy", (lossy, 1.0));
    let mut recycle = AIndexMap::default();
    recycle.insert("b", -1.0);
    recycle.insert("a", 1.0);
    flows.insert("recycle", (recycle, 0.5));
    let mut target = AIndexMap::default();
    target.insert("b", 1.0);

    let solution = SolverData::new_simple(target, flows).solve();
    assert!(
        matches!(solution, SolverSolution::NotSolved { .. }),
        "无外部输入时损耗循环净产出 b 应不可行"
    );
}

#[test]
fn solve_recycling_loop_with_external_supply() {
    // 同一循环 + 外部补给 a（成本 1.0/单位）：
    // balance_b = L - R = 1 → R = L - 1（R ≥ 0 → L ≥ 1）
    // balance_a = -2L + R + S ≥ 0 → S ≥ L + 1
    // 最小化 L + 0.5R + S = 2.5L + 0.5 → L = 1, R = 0, S = 2, cost = 3.0
    let mut flows = AIndexMap::default();
    let mut lossy = AIndexMap::default();
    lossy.insert("a", -2.0);
    lossy.insert("b", 1.0);
    flows.insert("lossy", (lossy, 1.0));
    let mut recycle = AIndexMap::default();
    recycle.insert("b", -1.0);
    recycle.insert("a", 1.0);
    flows.insert("recycle", (recycle, 0.5));
    let mut target = AIndexMap::default();
    target.insert("b", 1.0);
    let mut sources = AIndexMap::default();
    sources.insert("a", 1.0);

    let solution = SolverData::new_simple(target, flows)
        .with_sources(sources)
        .solve();
    match solution {
        SolverSolution::Solved {
            prim, sum, cost, ..
        } => {
            assert!((prim["lossy"] - 1.0).abs() < 1e-5, "lossy: {prim:?}");
            assert!((prim["recycle"]).abs() < 1e-5, "recycle: {prim:?}");
            assert!((sum["a"] + 2.0).abs() < 1e-5, "sum[a]: {sum:?}");
            assert!((sum["b"] - 1.0).abs() < 1e-5, "sum[b]: {sum:?}");
            assert!((cost - 3.0).abs() < 1e-5, "cost: {cost}");
        }
        SolverSolution::NotSolved { description, .. } => panic!("求解失败: {description}"),
    }
}
