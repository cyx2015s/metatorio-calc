//! TempRecipe 分裂式临时配方测试：固定流共享、变温流体自动分裂、
//! 端点不排序（温度相关性）、单点不分裂。

use metatorio_core::context::{Context, GameState};
use metatorio_core::dual_var::DualVar;
use metatorio_core::temp_flow::TempFlow;
use metatorio_data::store::PrototypeStore;
use serde_json::{Value, json};

fn _ctx_unused() {}
// （各测试内局部构造 Context）

fn store() -> PrototypeStore {
    let dump: Value = json!({
        "fluid": {
            "water": { "default_temperature": 15.0 },
            "steam": { "default_temperature": 100.0 },
            "crude-oil": { "default_temperature": 25.0, "heat_capacity": "1kJ" }
        }
    });
    PrototypeStore::load(&dump).expect("dump 加载失败")
}

#[test]
fn single_point_fluid_does_not_split() {
    let s = store();
    let game = GameState::default();
    let ctx = Context::new(&s, &game);
    let mut t = TempFlow::new();
    // 单点温度（lo == hi）→ 不分裂
    t.add_fluid(&ctx, "water", -10.0, 15.0, 15.0);
    let vars = t.into_variables(0);
    assert_eq!(vars.len(), 1);
}

#[test]
fn two_fluids_split_into_four_copies() {
    let s = store();
    let game = GameState::default();
    let ctx = Context::new(&s, &game);
    let mut t = TempFlow::new();
    // 两个变温流体 → 2 × 2 = 4 副本
    t.add_fluid(&ctx, "water", -10.0, 15.0, 500.0);
    t.add_fluid(&ctx, "crude-oil", -5.0, 25.0, 400.0);
    let vars = t.into_variables(0);
    assert_eq!(vars.len(), 4);
    // 变量顺序 = 副本顺序（位置即温度组合端序号）
    for (mask, v) in vars.iter().enumerate() {
        // 所有副本都有 water 与 crude-oil 的流体消耗
        assert_eq!(
            v.flow.get(&DualVar::Fluid {
                name: "water".into()
            }),
            Some(&-10.0)
        );
        assert_eq!(
            v.flow.get(&DualVar::Fluid {
                name: "crude-oil".into()
            }),
            Some(&-5.0)
        );
        // 热量随 mask 位变化（分裂是追加过程：最后添加的 crude-oil 在 bit0，water 在 bit1）：
        // water 端点 = bit1（0 → 15°C，1 → 500°C）
        let water_heat = if mask & 2 == 0 {
            0.0
        } else {
            -10.0 * (500.0 - 15.0) * 1000.0
        };
        assert_eq!(
            v.flow
                .get(&DualVar::FluidHeat {
                    filter: "water".into()
                })
                .unwrap_or(&0.0),
            &water_heat
        );
        // crude-oil 端点 = bit0（0 → 25°C，1 → 400°C）
        let oil_heat = if mask & 1 == 0 {
            0.0
        } else {
            -5.0 * (400.0 - 25.0) * 1000.0
        };
        assert_eq!(
            v.flow
                .get(&DualVar::FluidHeat {
                    filter: "crude-oil".into()
                })
                .unwrap_or(&0.0),
            &oil_heat
        );
    }
}

#[test]
fn reversed_endpoints_express_correlation() {
    let s = store();
    let game = GameState::default();
    let ctx = Context::new(&s, &game);
    let mut t = TempFlow::new();
    // lo > hi 合法：端点 0 = 500°C、端点 1 = 15°C（反向区间，表达相关性）
    t.add_fluid(&ctx, "water", -10.0, 500.0, 15.0);
    let vars = t.into_variables(0);
    assert_eq!(vars.len(), 2);
    // mask=0 → 端点 lo = 500°C
    assert_eq!(
        vars[0].flow.get(&DualVar::FluidHeat {
            filter: "water".into()
        }),
        Some(&(-10.0 * (500.0 - 15.0) * 1000.0))
    );
    // mask=1 → 端点 hi = 15°C（0 热量）
    assert_eq!(
        vars[1]
            .flow
            .get(&DualVar::FluidHeat {
                filter: "water".into()
            })
            .unwrap_or(&0.0),
        &0.0
    );
}

#[test]
fn fixed_flow_applies_to_all_copies() {
    let s = store();
    let game = GameState::default();
    let ctx = Context::new(&s, &game);
    let mut t = TempFlow::new();
    // 固定流先加、后加都作用到所有副本
    t.add(DualVar::Item("coal".into()), -2.0);
    t.add_fluid(&ctx, "water", -10.0, 15.0, 500.0);
    t.add(DualVar::Electricity, 1000.0);
    let vars = t.into_variables(0);
    assert_eq!(vars.len(), 2);
    for v in &vars {
        assert_eq!(v.flow.get(&DualVar::Item("coal".into())), Some(&-2.0));
        assert_eq!(v.flow.get(&DualVar::Electricity), Some(&1000.0));
    }
}

#[test]
fn add_dual_expresses_correlation() {
    let s = store();
    let game = GameState::default();
    let ctx = Context::new(&s, &game);
    let mut t = TempFlow::new();
    // 锅炉场景：水温不同 → 燃料消耗不同（相关性：lo 端水温 15°C 需更多煤，hi 端 500°C 少煤）
    let mut lo: metatorio_core::prim_var::Flow = Default::default();
    lo.insert(DualVar::Fluid { name: "water".into() }, -10.0);
    lo.insert(DualVar::Item("coal".into()), -2.0);
    let mut hi: metatorio_core::prim_var::Flow = Default::default();
    hi.insert(DualVar::Fluid { name: "water".into() }, -10.0);
    hi.insert(DualVar::Item("coal".into()), -1.0);
    t.add_dual(&lo, &hi);
    let vars = t.into_variables(0);
    assert_eq!(vars.len(), 2);
    // 副本 0 = lo 端：燃料消耗 2
    assert_eq!(vars[0].flow.get(&DualVar::Item("coal".into())), Some(&-2.0));
    // 副本 1 = hi 端：燃料消耗 1
    assert_eq!(vars[1].flow.get(&DualVar::Item("coal".into())), Some(&-1.0));
    // 两端共享的水量
    for v in &vars {
        assert_eq!(v.flow.get(&DualVar::Fluid { name: "water".into() }), Some(&-10.0));
    }
}
