//! 配置 → 虚拟流展开测试：物品配方、流体插值 2 端、调用方稳定键序。

use metatorio_core::context::{Context, GameState};
use metatorio_core::dual_var::DualVar;
use metatorio_core::expand::expand;
use metatorio_core::mechanic::{Mechanic, RecipeMechanic};
use metatorio_core::prim_var::AIndexMap;
use metatorio_data::store::PrototypeStore;
use serde_json::{Value, json};

fn load(dump: Value) -> PrototypeStore {
    PrototypeStore::load(&dump).expect("dump 加载失败")
}

/// 动态构造含流体配方的最小 dump。
fn fluid_dump() -> Value {
    json!({
        "fluid": {
            "water": { "default_temperature": 15.0 },
            "steam": { "default_temperature": 100.0, "heat_capacity": "1kJ" }
        },
        "recipe": {
            "iron-plate": {
                "ingredients": [{ "type": "item", "name": "iron-ore", "amount": 1 }],
                "results": [{ "type": "item", "name": "iron-plate", "amount": 1 }]
            },
            "heat-water": {
                "ingredients": [{ "type": "fluid", "name": "water", "amount": 10,
                                  "minimum_temperature": 15, "maximum_temperature": 500 }],
                "results": [{ "type": "fluid", "name": "steam", "amount": 10, "temperature": 165 }]
            }
        }
    })
}

fn recipe_mechanic(recipe: &str) -> Mechanic {
    Mechanic::Recipe(RecipeMechanic {
        recipe: recipe.into(),
        machine: "assembling-machine-1".into(),
        module_config: Default::default(),
        fuel: None,
    })
}

#[test]
fn item_recipe_expands_single_variable() {
    let store = load(fluid_dump());
    let game = GameState::default();
    let ctx = Context::new(&store, &game);
    let expansion = expand(
        [("iron-plate".to_string(), recipe_mechanic("iron-plate"))]
            .iter()
            .map(|(c, m)| (c.clone(), m)),
        &ctx,
    );

    assert_eq!(expansion.len(), 1);
    let v = &expansion.variables[0];
    assert_eq!(v.prim_var.inner, "iron-plate");
    assert_eq!(v.flow.get(&DualVar::Item("iron-ore".into())), Some(&-1.0));
    assert_eq!(v.flow.get(&DualVar::Item("iron-plate".into())), Some(&1.0));
}

#[test]
fn fluid_recipe_expands_two_interpolation_ends() {
    let store = load(fluid_dump());
    let game = GameState::default();
    let ctx = Context::new(&store, &game);
    let expansion = expand(
        [("heat-water".to_string(), recipe_mechanic("heat-water"))]
            .iter()
            .map(|(c, m)| (c.clone(), m)),
        &ctx,
    );

    // k=1 个流体输入 → 2 个插值端
    assert_eq!(expansion.len(), 2);
    let low = &expansion.variables[0];
    let high = &expansion.variables[1];
    assert_eq!(
        low.prim_var.inner, high.prim_var.inner,
        "同配置共享 config 标识"
    );

    // 两端共享：Fluid 流相同（水 -10、蒸汽 +10）
    for v in [low, high] {
        assert_eq!(
            v.flow.get(&DualVar::Fluid {
                name: "water".into()
            }),
            Some(&-10.0)
        );
        assert_eq!(
            v.flow.get(&DualVar::Fluid {
                name: "steam".into()
            }),
            Some(&10.0)
        );
        // 产物蒸汽 165°C：热量 = 10 × (165-100) × 1000 = 650 kJ
        assert_eq!(
            v.flow.get(&DualVar::FluidHeat {
                filter: "steam".into()
            }),
            Some(&650_000.0)
        );
    }

    // 低端（15°C 水）：热量 = 10 × (15-15) × 1000 = 0（水默认温度，无热量；0 系数被稀疏化跳过）
    assert_eq!(
        low.flow
            .get(&DualVar::FluidHeat {
                filter: "water".into()
            })
            .unwrap_or(&0.0),
        &0.0
    );
    // 高端（500°C 水）：热量 = 10 × (500-15) × 1000 = 4,850 kJ（消耗）
    assert_eq!(
        high.flow.get(&DualVar::FluidHeat {
            filter: "water".into()
        }),
        Some(&-4_850_000.0)
    );
}

#[test]
fn stable_key_order_from_caller() {
    let store = load(fluid_dump());
    let game = GameState::default();
    let ctx = Context::new(&store, &game);

    // 调用方持久结构：AIndexMap<config ID, Mechanic> 键序 = 稳定标识（UI 拖动不改变键序）
    let mut configs: AIndexMap<String, Mechanic> =
        AIndexMap::with_hasher(ahash::RandomState::default());
    configs.insert("iron-plate".to_string(), recipe_mechanic("iron-plate"));
    configs.insert("heat-water".to_string(), recipe_mechanic("heat-water"));

    let a = expand(configs.iter().map(|(k, m)| (k.clone(), m)), &ctx);
    // 同一键序再次展开 → 完全一致（求解结果可缓存不重算）
    let b = expand(configs.iter().map(|(k, m)| (k.clone(), m)), &ctx);
    assert_eq!(a, b);

    // config 标识 = 调用方的 ID：iron-plate 与 heat-water 各自独立
    let configs_used: Vec<&String> = a.variables.iter().map(|v| &v.prim_var.inner).collect();
    assert_eq!(configs_used, vec!["iron-plate", "heat-water", "heat-water"]);
    assert_eq!(
        a.variables[1].prim_var.inner, a.variables[2].prim_var.inner,
        "两个插值端共享 config"
    );
}
