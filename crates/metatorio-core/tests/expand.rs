//! 配置 → 虚拟流展开测试：物品配方、流体温度区间、调用方稳定键序。

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
        "assembling-machine": {
            "assembling-machine-1": {
                "crafting_speed": 1.0,
                "energy_usage": "0J",
                "energy_source": { "type": "electric", "drain": "0J" }
            }
        },
        "fluid": {
            "water": { "default_temperature": 15.0 },
            "steam": { "default_temperature": 100.0, "heat_capacity": "1kJ" }
        },
        "recipe": {
            "iron-plate": {
                "energy_required": 1.0,
                "ingredients": [{ "type": "item", "name": "iron-ore", "amount": 1 }],
                "results": [{ "type": "item", "name": "iron-plate", "amount": 1 }]
            },
            "heat-water": {
                "energy_required": 1.0,
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
        fuel_temperature: None,
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

    // 流体区间仍是一个由调用方/求解器转换的子类型，不由 TempFlow 自动拆分。
    let configs_used: Vec<&String> = a.variables.iter().map(|v| &v.prim_var.inner).collect();
    assert_eq!(configs_used, vec!["iron-plate", "heat-water"]);
    assert_eq!(
        a.variables[1].flow.get(&DualVar::Fluid {
            name: "water".to_string(),
            temperature: [15, 500],
        }),
        Some(&-10.0)
    );
    assert!(!a.variables[1].flow.keys().any(|key| matches!(
        key,
        DualVar::FluidHeat { filter } if filter == "water"
    )));
}
