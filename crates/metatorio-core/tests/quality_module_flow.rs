//! mechanic_flow 品质模块与品质变体展开回归验证（自包含合成原型夹具，不依赖本机游戏目录）。
use metatorio_core::context::{Context, GameState};
use metatorio_core::dual_var::DualVar;
use metatorio_core::expand::expand;
use metatorio_core::mechanic::{Mechanic, RecipeMechanic};
use metatorio_core::{IdWithQuality, ModuleConfig};
use metatorio_data::store::PrototypeStore;
use serde_json::json;

fn synthetic_quality_store() -> PrototypeStore {
    let dump = json!({
        "quality": {
            "normal": {
                "type": "quality",
                "name": "normal",
                "level": 0,
                "order": "a",
                "next": "uncommon",
                "next_probability": 0.1
            },
            "uncommon": {
                "type": "quality",
                "name": "uncommon",
                "level": 1,
                "order": "b",
                "next": "rare",
                "next_probability": 0.1
            },
            "rare": {
                "type": "quality",
                "name": "rare",
                "level": 2,
                "order": "c",
                "next": "epic",
                "next_probability": 0.1
            },
            "epic": {
                "type": "quality",
                "name": "epic",
                "level": 3,
                "order": "d",
                "next": "legendary",
                "next_probability": 0.1
            },
            "legendary": {
                "type": "quality",
                "name": "legendary",
                "level": 4,
                "order": "e"
            }
        },
        "item": {
            "iron-plate": { "type": "item", "name": "iron-plate" },
            "copper-cable": { "type": "item", "name": "copper-cable" },
            "electronic-circuit": { "type": "item", "name": "electronic-circuit" }
        },
        "module": {
            "quality-module-2": {
                "name": "quality-module-2",
                "type": "module",
                "category": "quality",
                "tier": 2,
                "effect": { "quality": 0.02 }
            }
        },
        "recipe": {
            "electronic-circuit": {
                "type": "recipe",
                "name": "electronic-circuit",
                "category": "electronics",
                "energy_required": 0.5,
                "ingredients": [
                    { "type": "item", "name": "iron-plate", "amount": 1 },
                    { "type": "item", "name": "copper-cable", "amount": 3 }
                ],
                "results": [
                    { "type": "item", "name": "electronic-circuit", "amount": 1 }
                ],
                "allow_quality": true
            }
        },
        "assembling-machine": {
            "chemical-plant": {
                "name": "chemical-plant",
                "type": "assembling-machine",
                "crafting_speed": 1.0,
                "crafting_categories": ["electronics"],
                "module_slots": 3,
                "allowed_effects": ["speed", "productivity", "consumption", "pollution", "quality"],
                "energy_usage": "210kW",
                "energy_source": { "type": "electric", "usage_priority": "secondary-input" }
            },
            "assembling-machine-2": {
                "name": "assembling-machine-2",
                "type": "assembling-machine",
                "crafting_speed": 0.75,
                "crafting_categories": ["electronics"],
                "module_slots": 2,
                "energy_usage": "150kW",
                "energy_source": { "type": "electric", "usage_priority": "secondary-input" }
            }
        }
    });
    PrototypeStore::load(&dump).expect("合成 dump 加载失败")
}

#[test]
fn real_dump_recipe_with_quality_module_has_flow() {
    let store = synthetic_quality_store();
    // 找一个配方（如 electronic-circuit）+ 装配机-1 + 品质插件。
    let game = GameState {
        qualities: store.quality_order().to_vec(),
        max_quality: store.quality_order().len().saturating_sub(1),
        ..Default::default()
    };
    let ctx = Context::new(&store, &game);
    // chemical-plant 允许 quality 效果（真实 dump），品质插件效果应生效。
    let mechanic = Mechanic::Recipe(RecipeMechanic {
        recipe: IdWithQuality::new("electronic-circuit", "normal"),
        machine: IdWithQuality::new("chemical-plant", "normal"),
        module_config: ModuleConfig {
            modules: vec![IdWithQuality::new("quality-module-2", "normal")],
            beacons: vec![],
        },
        fuel: None,
    });
    let expansion = expand([(0usize, &mechanic)], &ctx);
    assert_eq!(expansion.len(), 1, "应产出一个变量");
    let flow = &expansion.variables[0].flow;
    let item_flows: Vec<_> = flow
        .iter()
        .filter(|(flow, _)| matches!(flow, DualVar::Item(_)))
        .collect();
    assert!(
        !item_flows.is_empty(),
        "带品质插件的配方应产出物品流，实际 flow={flow:?}"
    );
    // 电子电路产出应包含 normal + 至少一个升级品质（品质插件生效）。
    let circuit_flows: Vec<_> = item_flows
        .iter()
        .filter(|(flow, _)| matches!(flow, DualVar::Item(i) if i.id == "electronic-circuit"))
        .collect();
    assert!(
        circuit_flows.len() >= 2,
        "品质插件应把电子电路拆分为多品质流：{circuit_flows:?}"
    );
    for (flow, amount) in &circuit_flows {
        let DualVar::Item(item) = flow else {
            unreachable!()
        };
        eprintln!("电路 {}/{} 产量 {amount}", item.id, item.quality);
    }
}

/// 模拟 mechanic_flow 命令完整路径（make_game_state + expand + 合并流）：
/// 品质变体（recipe.quality=uncommon）在品质上限开启时产出对应品质流。
#[test]
fn real_dump_quality_variant_recipe_has_flow() {
    let store = synthetic_quality_store();
    let game = GameState {
        qualities: store.quality_order().to_vec(),
        max_quality: store.quality_order().len().saturating_sub(1),
        ..Default::default()
    };
    let ctx = Context::new(&store, &game);
    // 配方品质 = uncommon（品质变体机制条目）。
    let mechanic = Mechanic::Recipe(RecipeMechanic {
        recipe: IdWithQuality::new("electronic-circuit", "uncommon"),
        machine: IdWithQuality::new("assembling-machine-2", "normal"),
        module_config: ModuleConfig::default(),
        fuel: None,
    });
    let expansion = expand([(0usize, &mechanic)], &ctx);
    assert_eq!(expansion.len(), 1, "应产出一个变量");
    let flow = &expansion.variables[0].flow;
    let outputs: Vec<_> = flow
        .iter()
        .filter(|(flow, amount)| matches!(flow, DualVar::Item(_)) && **amount > 0.0)
        .collect();
    assert!(
        !outputs.is_empty(),
        "品质变体配方应产出物品流，实际 flow={flow:?}"
    );
    for (flow, amount) in &outputs {
        let DualVar::Item(item) = flow else {
            unreachable!()
        };
        eprintln!("产出 {}/{} 产量 {amount}", item.id, item.quality);
    }
}

/// 模拟项目默认设置（品质上限 0）：加品质插件后仍应产出物品流（非空）。
#[test]
fn quality_module_with_zero_limit_still_has_flow() {
    let store = synthetic_quality_store();
    // all_accessible=false 默认 → max_quality=0（品质未开启）。
    let game = GameState {
        qualities: store.quality_order().to_vec(),
        max_quality: 0,
        ..Default::default()
    };
    let ctx = Context::new(&store, &game);
    let mechanic = Mechanic::Recipe(RecipeMechanic {
        recipe: IdWithQuality::new("electronic-circuit", "normal"),
        machine: IdWithQuality::new("chemical-plant", "normal"),
        module_config: ModuleConfig {
            modules: vec![IdWithQuality::new("quality-module-2", "normal")],
            beacons: vec![],
        },
        fuel: None,
    });
    let expansion = expand([(0usize, &mechanic)], &ctx);
    assert_eq!(expansion.len(), 1);
    let flow = &expansion.variables[0].flow;
    let outputs: Vec<_> = flow
        .iter()
        .filter(|(flow, amount)| matches!(flow, DualVar::Item(_)) && **amount > 0.0)
        .collect();
    assert!(
        !outputs.is_empty(),
        "品质上限 0 时带品质插件的配方也应产出物品流：flow={flow:?}"
    );
    for (flow, amount) in &outputs {
        let DualVar::Item(item) = flow else {
            unreachable!()
        };
        eprintln!("品质上限0 产出 {}/{} 产量 {amount}", item.id, item.quality);
    }
}

/// 品质变体（recipe.quality=uncommon）+ 品质插件：应产出多品质流。
#[test]
fn quality_variant_with_quality_module_has_multi_quality_flow() {
    let store = synthetic_quality_store();
    let game = GameState {
        qualities: store.quality_order().to_vec(),
        max_quality: store.quality_order().len().saturating_sub(1),
        ..Default::default()
    };
    let ctx = Context::new(&store, &game);
    let mechanic = Mechanic::Recipe(RecipeMechanic {
        recipe: IdWithQuality::new("electronic-circuit", "uncommon"),
        machine: IdWithQuality::new("chemical-plant", "normal"),
        module_config: ModuleConfig {
            modules: vec![IdWithQuality::new("quality-module-2", "normal")],
            beacons: vec![],
        },
        fuel: None,
    });
    let expansion = expand([(0usize, &mechanic)], &ctx);
    assert_eq!(expansion.len(), 1);
    let flow = &expansion.variables[0].flow;
    let circuit_flows: Vec<_> = flow
        .iter()
        .filter(|(flow, amount)| {
            matches!(flow, DualVar::Item(i) if i.id == "electronic-circuit") && **amount > 0.0
        })
        .collect();
    assert!(
        circuit_flows.len() >= 2,
        "品质变体 + 品质插件应产出多品质电路流：{circuit_flows:?}"
    );
    for (flow, amount) in &circuit_flows {
        let DualVar::Item(item) = flow else {
            unreachable!()
        };
        eprintln!("变体+插件 产出 {}/{} 产量 {amount}", item.id, item.quality);
    }
}
