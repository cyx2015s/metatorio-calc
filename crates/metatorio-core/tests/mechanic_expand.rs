//! Recipe/Mining 完整展开测试：机器能耗、速度、品质、矿脉消耗。
//!
//! dump 数据（手算基准）：
//! - assembling-machine-1：crafting_speed 0.5、energy_usage "90kW"（= 1500 J/tick）
//! - electric-mining-drill：mining_speed 0.5、energy_usage "90kW"
//! - iron-plate 配方：energy_required 3.2、1 iron-ore → 1 iron-plate
//! - iron-ore 矿脉实体：mining_time 2、result iron-ore
//! - 无模块/插件塔/品质原型（品质分布防御 → [1.0]）

use metatorio_core::context::{Context, GameState};
use metatorio_core::dual_var::DualVar;
use metatorio_core::expand::expand;
use metatorio_core::mechanic::{Mechanic, MiningMechanic, RecipeMechanic};
use metatorio_data::store::PrototypeStore;
use serde_json::{Value, json};

fn dump() -> Value {
    json!({
        "assembling-machine": {
            "assembling-machine-1": {
                "crafting_speed": 0.5,
                "energy_usage": "90kW",
                "energy_source": { "type": "electric" }
            }
        },
        "mining-drill": {
            "electric-mining-drill": {
                "mining_speed": 0.5,
                "energy_usage": "90kW",
                "energy_source": { "type": "electric" }
            }
        },
        "recipe": {
            "iron-plate": {
                "energy_required": 3.2,
                "ingredients": [{ "type": "item", "name": "iron-ore", "amount": 1 }],
                "results": [{ "type": "item", "name": "iron-plate", "amount": 1 }]
            }
        },
        "resource": {
            "iron-ore": {
                "minable": { "mining_time": 2.0, "result": "iron-ore" }
            }
        }
    })
}

#[test]
fn recipe_full_expansion_with_energy() {
    let store = PrototypeStore::load(&dump()).expect("dump 加载失败");
    let game = GameState::default();
    let ctx = Context::new(&store, &game);

    let m = Mechanic::Recipe(RecipeMechanic {
        recipe: "iron-plate".into(),
        machine: "assembling-machine-1".into(),
        module_config: Default::default(),
        fuel: None,
    });
    let expansion = expand([("r".to_string(), &m)].into_iter(), &ctx);
    assert_eq!(expansion.len(), 1, "无流体输入，单变量");
    let flow = &expansion.variables[0].flow;

    // base_speed = 0.5（crafting_speed）× 1.0（品质倍率）÷ 3.2（energy_required）= 0.15625
    let base_speed = 0.5 / 3.2;
    // 原料：-1 × (1+0) × base_speed
    assert_eq!(
        flow.get(&DualVar::Item("iron-ore".into())),
        Some(&-base_speed)
    );
    // 产物（normal 品质全概率）
    assert_eq!(
        flow.get(&DualVar::Item("iron-plate".into())),
        Some(&base_speed)
    );
    // 能耗：90kW = 1500 J/tick → ×60 = 90000 J/s；无 drain → 额外 -1500×60/30 = -3000
    assert_eq!(
        flow.get(&DualVar::Electricity),
        Some(&(-90_000.0 - 3_000.0))
    );
}

#[test]
fn mining_full_expansion_with_energy() {
    let store = PrototypeStore::load(&dump()).expect("dump 加载失败");
    let game = GameState::default();
    let ctx = Context::new(&store, &game);

    let m = Mechanic::Mining(MiningMechanic {
        resource: "iron-ore".into(),
        machine: "electric-mining-drill".into(),
        module_config: Default::default(),
        fuel: None,
    });
    let expansion = expand([("m".to_string(), &m)].into_iter(), &ctx);
    assert_eq!(expansion.len(), 1);
    let flow = &expansion.variables[0].flow;

    // base_speed = 0.5（mining_speed）÷ 2（mining_time）= 0.25
    let base_speed = 0.5 / 2.0;
    // 矿脉实体消耗：-0.25 × 1 × drain_rate(1.0)
    assert_eq!(
        flow.get(&DualVar::Entity("iron-ore".into())),
        Some(&-base_speed)
    );
    // 产物：count=1、productivity=0 → 0.25 iron-ore/s（normal）
    assert_eq!(
        flow.get(&DualVar::Item("iron-ore".into())),
        Some(&base_speed)
    );
    // 能耗：与 recipe 相同
    assert_eq!(
        flow.get(&DualVar::Electricity),
        Some(&(-90_000.0 - 3_000.0))
    );
}

#[test]
fn recipe_with_fuel_fluid() {
    // 流体供能的机器：指定燃料流体（FuelSpec::Fluid）
    let store = PrototypeStore::load(&json!({
        "fluid": { "water": { "default_temperature": 15.0, "fuel_value": "10kJ" } },
        "assembling-machine": {
            "fluid-assembler": {
                "crafting_speed": 1.0,
                "energy_usage": "90kW",
                "energy_source": { "type": "fluid", "burns_fluid": true, "fluid_usage_per_tick": 1.0, "effectivity": 1.0 }
            }
        },
        "recipe": {
            "iron-plate": {
                "energy_required": 1.0,
                "ingredients": [{ "type": "item", "name": "iron-ore", "amount": 1 }],
                "results": [{ "type": "item", "name": "iron-plate", "amount": 1 }]
            }
        }
    }))
    .expect("dump 加载失败");
    let game = GameState::default();
    let ctx = Context::new(&store, &game);

    let m = Mechanic::Recipe(RecipeMechanic {
        recipe: "iron-plate".into(),
        machine: "fluid-assembler".into(),
        module_config: Default::default(),
        fuel: Some("water".to_string()),
    });
    let expansion = expand([("r".to_string(), &m)].into_iter(), &ctx);
    let flow = &expansion.variables[0].flow;

    // 燃料消耗：90000 J/s ÷ 10000 J/unit = 9 unit/s，但流体源不可变流量（scale_fluid_usage 缺省 false）
    // → 至少要满足指定流量：fluid_usage_per_tick × 60 = 60 unit/s（egui 语义，忠实迁移）
    assert_eq!(
        flow.get(&DualVar::Fluid {
            name: "water".into()
        }),
        Some(&-60.0)
    );
    // 燃料携带热量：9 × (15-15) × 1000 = 0（默认温度，跳过）
    assert_eq!(
        flow.get(&DualVar::FluidHeat {
            filter: "water".into()
        })
        .unwrap_or(&0.0),
        &0.0
    );
    // 产物正常
    assert_eq!(flow.get(&DualVar::Item("iron-plate".into())), Some(&1.0));
}
