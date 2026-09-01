use metatorio_core::IdWithQuality;
use metatorio_core::context::{Context, GameState};
use metatorio_core::dual_var::DualVar;
use metatorio_core::expand::expand;
use metatorio_core::mechanic::{
    BoilerMechanic, GeneratorMechanic, ItemFuelMechanic, ItemLaunchMechanic, Mechanic,
    PlantMechanic, ReactorMechanic, SpoilMechanic,
};
use metatorio_data::store::PrototypeStore;
use serde_json::{Value, json};

fn id(name: &str) -> IdWithQuality {
    IdWithQuality::new(name, "normal")
}

fn flow(dump: Value, mechanic: Mechanic) -> metatorio_core::prim_var::Flow {
    let store = PrototypeStore::load(&dump).expect("dump should load");
    let game = GameState {
        max_quality: store.quality_order().len().saturating_sub(1),
        ..Default::default()
    };
    flow_loaded(store, game, mechanic)
}

fn flow_with_game(
    dump: Value,
    mechanic: Mechanic,
    game: GameState,
) -> metatorio_core::prim_var::Flow {
    let store = PrototypeStore::load(&dump).expect("dump should load");
    flow_loaded(store, game, mechanic)
}

fn flow_loaded(
    store: PrototypeStore,
    game: GameState,
    mechanic: Mechanic,
) -> metatorio_core::prim_var::Flow {
    let ctx = Context::new(&store, &game);
    let expansion = expand([(0usize, &mechanic)], &ctx);
    assert_eq!(expansion.len(), 1, "mechanic should produce one variable");
    expansion.variables.into_iter().next().unwrap().flow
}

#[test]
fn item_fuel_preserves_burnt_result() {
    let flow = flow(
        json!({
            "item": {
                "coal": {
                    "fuel_value": "8MJ",
                    "fuel_category": "chemical",
                    "burnt_result": "ash"
                }
            }
        }),
        Mechanic::ItemFuel(ItemFuelMechanic { item: id("coal") }),
    );

    assert_eq!(flow[&DualVar::Item(id("coal"))], -1.0);
    assert_eq!(
        flow[&DualVar::ItemFuel {
            category: vec!["chemical".to_string()],
            has_burnt_result: true,
        }],
        8_000_000.0
    );
    assert_eq!(flow[&DualVar::Item(id("ash"))], 1.0);
}

#[test]
fn spoil_changes_quality_and_consumes_the_source() {
    let flow = flow(
        json!({
            "item": {
                "fresh": {
                    "spoil_ticks": 60,
                    "spoil_result": "spoiled",
                    "spoil_quality_change": 1
                }
            },
            "quality": {
                "normal": { "type": "quality", "name": "normal", "level": 0, "next": "uncommon", "next_probability": 1.0 },
                "uncommon": { "type": "quality", "name": "uncommon", "level": 1, "next": "rare", "next_probability": 1.0 },
                "rare": { "type": "quality", "name": "rare", "level": 2, "next": "epic", "next_probability": 1.0 },
                "epic": { "type": "quality", "name": "epic", "level": 3, "next": "legendary", "next_probability": 1.0 },
                "legendary": { "type": "quality", "name": "legendary", "level": 4 }
            }
        }),
        Mechanic::Spoil(SpoilMechanic { item: id("fresh") }),
    );

    dbg!(&flow);
    assert_eq!(flow[&DualVar::Item(id("fresh"))], -1.0);
    assert_eq!(
        flow[&DualVar::Item(IdWithQuality::new("spoiled", "uncommon"))],
        1.0
    );
}

#[test]
fn plant_is_a_per_second_cycle() {
    let flow = flow(
        json!({
            "item": { "seed": { "plant_result": "plant" } },
            "plant": {
                "plant": {
                    "growth_ticks": 60,
                    "harvest_emissions": { "pollution": 2.0 },
                    "minable": { "mining_time": 1.0, "result": "fruit", "count": 2 }
                }
            }
        }),
        Mechanic::Plant(PlantMechanic { seed: id("seed") }),
    );

    assert_eq!(flow[&DualVar::Item(id("seed"))], -1.0);
    assert_eq!(flow[&DualVar::Item(id("fruit"))], 2.0);
    assert_eq!(
        flow[&DualVar::Pollution {
            name: "pollution".to_string(),
        }],
        2.0
    );
}

#[test]
fn generator_consumes_fluid_and_produces_electricity() {
    let flow = flow(
        json!({
            "fluid": { "steam": { "default_temperature": 100.0, "fuel_value": "1MJ" } },
            "generator": {
                "steam-engine": {
                    "fluid_box": { "filter": "steam" },
                    "fluid_usage_per_tick": 1.0,
                    "maximum_temperature": 500.0,
                    "burns_fluid": true,
                    "energy_source": {}
                }
            }
        }),
        Mechanic::Generator(GeneratorMechanic {
            generator: id("steam-engine"),
            fluid: "steam".to_string(),
            temperature: Some(100),
        }),
    );

    assert_eq!(
        flow[&DualVar::Fluid {
            name: "steam".to_string(),
            temperature: [100, 100],
        }],
        -60.0
    );
    assert_eq!(flow[&DualVar::Electricity], 60_000_000.0);
}

#[test]
fn boiler_output_mode_converts_fluid() {
    let flow = flow(
        json!({
            "fluid": {
                "water": { "default_temperature": 15.0, "heat_capacity": "1kJ" },
                "steam": { "default_temperature": 100.0, "heat_capacity": "1kJ" }
            },
            "boiler": {
                "boiler": {
                    "mode": "output-to-separate-pipe",
                    "target_temperature": 165.0,
                    "energy_consumption": "1MW",
                    "energy_source": { "type": "electric" },
                    "fluid_box": { "filter": "water" },
                    "output_fluid_box": { "filter": "steam" }
                }
            }
        }),
        Mechanic::Boiler(BoilerMechanic {
            boiler: id("boiler"),
            fluid: "water".to_string(),
            temperature: Some(15),
            fuel: None,
            fuel_temperature: None,
        }),
    );

    assert!(
        flow[&DualVar::Fluid {
            name: "water".to_string(),
            temperature: [15, 15],
        }] < 0.0
    );
    assert!(
        flow[&DualVar::Fluid {
            name: "steam".to_string(),
            temperature: [165, 165],
        }] > 0.0
    );
}

#[test]
fn reactor_outputs_heat() {
    let flow = flow(
        json!({
            "item": {
                "fuel": { "fuel_value": "1MJ", "fuel_category": "chemical" }
            },
            "reactor": {
                "reactor": {
                    "consumption": "1MW",
                    "neighbour_bonus": 1.0,
                    "energy_source": {
                        "type": "burner",
                        "fuel_categories": ["chemical"],
                        "effectivity": 1.0
                    },
                    "heat_buffer": {
                        "max_transfer": "10MW",
                        "max_temperature": 1000.0,
                        "specific_heat": "1MJ"
                    }
                }
            }
        }),
        Mechanic::Reactor(ReactorMechanic {
            reactor: id("reactor"),
            neighbours: 2,
            fuel: Some("fuel".to_string()),
        }),
    );

    assert!((flow[&DualVar::Item(id("fuel"))] + 1.0).abs() < 1e-12);
    assert!((flow[&DualVar::Heat] - 3_000_000.0).abs() < 1e-6);
}

#[test]
fn reactor_quality_uses_default_multiplier() {
    let game = GameState {
        qualities: vec!["normal".to_string(), "quality".to_string()],
        max_quality: 1,
        ..Default::default()
    };
    let flow = flow_with_game(
        json!({
            "quality": {
                "normal": { "level": 0, "next": "quality", "next_probability": 1.0 },
                "quality": { "level": 1, "default_multiplier": 2.0 }
            },
            "item": {
                "fuel": { "fuel_value": "1MJ", "fuel_category": "chemical" }
            },
            "reactor": {
                "reactor": {
                    "consumption": "1MW",
                    "energy_source": {
                        "type": "burner",
                        "fuel_categories": ["chemical"],
                        "effectivity": 1.0
                    },
                    "heat_buffer": {
                        "max_transfer": "10MW",
                        "max_temperature": 1000.0,
                        "specific_heat": "1MJ"
                    }
                }
            }
        }),
        Mechanic::Reactor(ReactorMechanic {
            reactor: IdWithQuality::new("reactor", "quality"),
            neighbours: 0,
            fuel: Some("fuel".to_string()),
        }),
        game,
    );

    assert!((flow[&DualVar::Item(IdWithQuality::new("fuel", "quality"))] + 2.0).abs() < 1e-12);
    assert!((flow[&DualVar::Heat] - 2_000_000.0).abs() < 1e-6);
}

#[test]
fn item_launch_uses_rocket_silo_capacity() {
    let flow = flow(
        json!({
            "item": {
                "satellite": {
                    "stack_size": 1,
                    "rocket_launch_products": [
                        { "type": "item", "name": "science", "amount": 100 }
                    ]
                }
            },
            "rocket-silo": {
                "silo": {
                    "launch_to_space_platforms": false,
                    "to_be_inserted_to_rocket_inventory_size": 10
                }
            }
        }),
        Mechanic::ItemLaunch(ItemLaunchMechanic {
            item: id("satellite"),
            weight_mode: false,
        }),
    );

    assert_eq!(flow[&DualVar::Item(id("satellite"))], -10.0);
    assert_eq!(flow[&DualVar::RocketSlotCapacity], -10.0);
    assert_eq!(flow[&DualVar::Item(id("science"))], 1000.0);
}

/// 回归：组装机为 rocket-silo 时，配方应额外产出火箭发射载荷（虚拟物品）。
/// 此前 expand_recipe 只按普通机器展开，导致 ItemLaunch 消耗的容量无来源。
#[test]
fn recipe_in_rocket_silo_produces_launch_capacity() {
    let flow = flow(
        json!({
            "item": {
                "rocket-part": { "type": "item", "name": "rocket-part", "stack_size": 10 },
                "space-part": { "type": "item", "name": "space-part", "stack_size": 10 }
            },
            "recipe": {
                "rocket-part": {
                    "type": "recipe", "name": "rocket-part",
                    "energy_required": 1, "enabled": true,
                    "ingredients": [{ "type": "item", "name": "rocket-part", "amount": 5 }],
                    "results": [{ "type": "item", "name": "space-part", "amount": 1 }]
                }
            },
            "rocket-silo": {
                "silo": {
                    "type": "rocket-silo", "name": "silo",
                    "energy_usage": "1MW",
                    "energy_source": { "type": "electric", "drain": "0J" },
                    "crafting_speed": 1, "crafting_categories": ["crafting"],
                    "launch_to_space_platforms": false,
                    "rocket_parts_required": 5,
                    "to_be_inserted_to_rocket_inventory_size": 10
                }
            }
        }),
        Mechanic::Recipe(metatorio_core::RecipeMechanic {
            recipe: IdWithQuality::new("rocket-part", "normal"),
            machine: IdWithQuality::new("silo", "normal"),
            module_config: Default::default(),
            fuel: None,
            fuel_temperature: None,
        }),
    );
    // 每次合成产出 整枚火箭容量(10) / rocket_parts_required(5) = 2.
    assert_eq!(flow[&DualVar::RocketSlotCapacity], 2.0);
    assert_eq!(flow[&DualVar::Item(id("space-part"))], 1.0);
    assert_eq!(flow[&DualVar::Item(id("rocket-part"))], -5.0);
}

/// 配方 + 品质插件：品质效果把产出拆分为多品质流（normal + 升级品质），
/// 配方机制卡应显示这些流量（回归：带品质插件的配方流量不显示）。
#[test]
fn recipe_with_quality_module_produces_multi_quality_flow() {
    let dump = json!({
        "item": {
            "iron-ore": { "type": "item", "name": "iron-ore", "stack_size": 50 },
            "iron-plate": { "type": "item", "name": "iron-plate", "stack_size": 100 },
            "quality-module": {
                "type": "item", "name": "quality-module",
                "category": "quality"
            }
        },
        "quality": {
            "normal": { "type": "quality", "name": "normal", "level": 0, "next": "uncommon", "next_probability": 1.0 },
            "uncommon": { "type": "quality", "name": "uncommon", "level": 1 }
        },
        "recipe": {
            "iron-plate": {
                "type": "recipe", "name": "iron-plate",
                "category": "smelting",
                "energy_required": 1,
                "ingredients": [{ "type": "item", "name": "iron-ore", "amount": 1 }],
                "results": [{ "type": "item", "name": "iron-plate", "amount": 1 }]
            }
        },
        "assembling-machine": {
            "assembling-machine-1": {
                "type": "assembling-machine", "name": "assembling-machine-1",
                "crafting_categories": ["smelting"], "crafting_speed": 1, "module_slots": 1,
                "energy_usage": "90kW",
                "energy_source": { "type": "electric", "drain": "0J" },
                "allowed_effects": ["speed", "productivity", "quality", "consumption", "pollution"]
            }
        },
        "module": {
            "quality-module": {
                "type": "module", "name": "quality-module",
                "category": "quality",
                "effect": { "quality": 0.5, "speed": -0.05, "consumption": 0.3 }
            }
        }
    });
    let mechanic = Mechanic::Recipe(metatorio_core::RecipeMechanic {
        recipe: IdWithQuality::new("iron-plate", "normal"),
        machine: IdWithQuality::new("assembling-machine-1", "normal"),
        module_config: metatorio_core::ModuleConfig {
            modules: vec![IdWithQuality::new("quality-module", "normal")],
            beacons: vec![],
        },
        fuel: None,
        fuel_temperature: None,
    });
    let store = PrototypeStore::load(&dump).expect("dump should load");
    let game = GameState {
        qualities: vec!["normal".to_string(), "uncommon".to_string()],
        max_quality: 1,
        ..Default::default()
    };
    let ctx = Context::new(&store, &game);
    let expansion = expand([(0usize, &mechanic)], &ctx);
    assert_eq!(expansion.len(), 1, "mechanic should produce one variable");
    let flow = &expansion.variables[0].flow;
    // 品质插件生效：产出含 normal 与 uncommon 两种品质的铁板流。
    let normal = flow
        .get(&DualVar::Item(IdWithQuality::new("iron-plate", "normal")))
        .copied()
        .unwrap_or(0.0);
    let uncommon = flow
        .get(&DualVar::Item(IdWithQuality::new("iron-plate", "uncommon")))
        .copied()
        .unwrap_or(0.0);
    assert!(
        normal > 0.0 && uncommon > 0.0,
        "品质插件应把产出拆分为多品质流：normal={normal} uncommon={uncommon}, flow={flow:?}"
    );
    // 品质分布各占一半（next_probability=1.0，quality=0.5 直接升级一级）。
    assert!(
        (normal - uncommon).abs() < 1e-9,
        "normal 与 uncommon 产出应相等：normal={normal} uncommon={uncommon}"
    );
    // 原料仍是 normal 铁矿石（配方品质 normal 输入）。
    assert!(
        flow.get(&DualVar::Item(IdWithQuality::new("iron-ore", "normal")))
            .is_some(),
        "应消耗 normal 铁矿石"
    );
}
