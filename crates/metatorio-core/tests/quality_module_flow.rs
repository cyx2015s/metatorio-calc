//! mechanic_flow 命令核心逻辑的真实 dump 验证：
//! 带品质插件的配方机制，展开流应包含多品质产出。
use metatorio_core::context::{Context, GameState};
use metatorio_core::dual_var::DualVar;
use metatorio_core::expand::expand;
use metatorio_core::mechanic::{Mechanic, RecipeMechanic};
use metatorio_core::{IdWithQuality, ModuleConfig};
use metatorio_data::store::PrototypeStore;

fn real_store() -> PrototypeStore {
    let path = "C:\\Users\\mirac\\AppData\\Roaming\\Factorio\\script-output\\data-raw-dump.json";
    if !std::path::Path::new(path).exists() {
        panic!("无真实 dump");
    }
    let raw = std::fs::read(path).expect("读 dump");
    let dump: serde_json::Value = serde_json::from_slice(&raw).expect("解析 dump");
    PrototypeStore::load(&dump).expect("dump 加载失败")
}

#[test]
fn real_dump_recipe_with_quality_module_has_flow() {
    let store = real_store();
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
    let store = real_store();
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
    let store = real_store();
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
    let store = real_store();
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
