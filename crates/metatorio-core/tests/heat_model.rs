//! 数学验证：同种流体输入输出不同温度的可解释性。
//!
//! 场景：输入 100 单位 100°C 水，输出 50 单位 50°C 水 + 1 单位固体。
//! 水：default_temperature = 15°C，heat_capacity = 1 kJ/(unit·°C)。
//!
//! 预期（手算）：
//! - Fluid{water}：−100 + 50 = −50（净消耗 50，由工厂其他水源平衡）
//! - FluidHeat{water}：−100×(100−15)×1000 + 50×(50−15)×1000 = −6,750,000 J
//!   （冷却 100→50°C 需移走 6.75 MJ，须由配方能量项对冲）
//! - Item{solid}：+1
//! - 温度回代：H_out/(m×c) + T0 = 1.75M/(50×1000) + 15 = 50°C ✓

use metatorio_core::context::{Context, GameState};
use metatorio_core::dual_var::DualVar;
use metatorio_core::temp_flow::TempFlow;
use metatorio_data::store::PrototypeStore;
use serde_json::{Value, json};

fn store() -> PrototypeStore {
    let dump: Value = json!({
        "fluid": {
            "water": { "default_temperature": 15.0, "heat_capacity": "1kJ" }
        }
    });
    PrototypeStore::load(&dump).expect("dump 加载失败")
}

#[test]
fn mixed_temperature_fluid_is_explainable() {
    let s = store();
    let game = GameState::default();
    let ctx = Context::new(&s, &game);

    let mut t = TempFlow::new();
    // 输入：100 单位 100°C 水（消耗）
    t.add_fluid(&ctx, "water", -100.0, 100.0, 100.0);
    // 输出：50 单位 50°C 水（产出）
    t.add_fluid(&ctx, "water", 50.0, 50.0, 50.0);
    // 输出：1 单位固体（不含热量）
    t.add(DualVar::Item("solid".into()), 1.0);

    let vars = t.into_variables("cooling");
    assert_eq!(vars.len(), 1, "全单点温度，不分裂");
    let flow = &vars[0].flow;

    // 流体质量守恒净值：−100 + 50 = −50（净消耗由工厂其他水源平衡）
    assert_eq!(
        flow.get(&DualVar::Fluid {
            name: "water".into()
        }),
        Some(&-50.0)
    );
    // 虚拟热量净值：−8.5M + 1.75M = −6.75 MJ
    assert_eq!(
        flow.get(&DualVar::FluidHeat {
            filter: "water".into()
        }),
        Some(&-6_750_000.0)
    );
    // 固体产出
    assert_eq!(flow.get(&DualVar::Item("solid".into())), Some(&1.0));

    // 温度回代自洽：输出端热量 1.75M / (50 × 1000) + 15 = 50°C
    let out_heat = 50.0 * (50.0 - 15.0) * 1000.0;
    let out_mass = 50.0;
    let implied_temp = out_heat / (out_mass * 1000.0) + 15.0;
    assert_eq!(implied_temp, 50.0);

    // 能量缺口显式：−6.75 MJ 必须由配方能量项（Heat/Electricity）对冲，
    // 否则 LP 暴露缺口——模型不静默丢失能量
    let heat_balance = flow
        .get(&DualVar::FluidHeat {
            filter: "water".into(),
        })
        .copied()
        .unwrap();
    // 若配方声明 Heat 产出 +6.75 MJ → 平衡（此处仅验证缺口的数值）
    assert_eq!(heat_balance, -6_750_000.0);
}

#[test]
fn merging_same_fluid_different_temperatures_averages() {
    let s = store();
    let game = GameState::default();
    let ctx = Context::new(&s, &game);

    // 两股同种流体汇合：50 单位 50°C + 50 单位 85°C
    let mut t = TempFlow::new();
    t.add_fluid(&ctx, "water", 50.0, 50.0, 50.0);
    t.add_fluid(&ctx, "water", 50.0, 85.0, 85.0);
    let vars = t.into_variables("merge");
    let flow = &vars[0].flow;

    // 质量合并 + 热量合并 → 隐式温度 = 加权平均
    assert_eq!(
        flow.get(&DualVar::Fluid {
            name: "water".into()
        }),
        Some(&100.0)
    );
    let total_heat = flow
        .get(&DualVar::FluidHeat {
            filter: "water".into(),
        })
        .copied()
        .unwrap();
    assert_eq!(
        total_heat,
        50.0 * (50.0 - 15.0) * 1000.0 + 50.0 * (85.0 - 15.0) * 1000.0
    );
    let implied_temp = total_heat / (100.0 * 1000.0) + 15.0;
    assert_eq!(implied_temp, 67.5, "加权平均温度");
}
