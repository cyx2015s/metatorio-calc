use metatorio_core::IdWithQuality;
use metatorio_core::context::{Context, GameState};
use metatorio_core::dual_var::DualVar;
use metatorio_core::expand::expand;
use metatorio_core::mechanic::{
    BoilerMechanic, Fuel, GeneratorMechanic, ItemFuelMechanic, ItemLaunchMechanic, Mechanic,
    PlantMechanic, ReactorMechanic, SpoilMechanic, TileDisposeMechanic, TileExtractMechanic,
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
        flow[&DualVar::ItemFuelSupply {
            category: vec!["chemical".to_string()],
            has_burnt_result: true,
        }],
        8_000_000.0
    );
    assert_eq!(flow[&DualVar::Item(id("ash"))], 1.0);
}

/// 多燃料类别物品：以完整类别集合作为**单个**供给键（一个变量），
/// 与 burner 的兼容性（两集合有重叠）由求解器的零成本转换流表达。
#[test]
fn item_fuel_keeps_multi_category_supply_key() {
    let dump = json!({
        "item": {
            "biofuel": {
                "fuel_value": "2MJ",
                "fuel_categories": ["chemical", "biological"],
                "burnt_result": "ash"
            }
        }
    });
    let store = PrototypeStore::load(&dump).expect("dump should load");
    let game = GameState {
        max_quality: store.quality_order().len().saturating_sub(1),
        ..Default::default()
    };
    let mechanic = Mechanic::ItemFuel(ItemFuelMechanic {
        item: id("biofuel"),
    });
    let flow = flow_loaded(store, game, mechanic);
    assert_eq!(flow[&DualVar::Item(id("biofuel"))], -1.0);
    assert_eq!(flow[&DualVar::Item(id("ash"))], 1.0);
    assert_eq!(
        flow[&DualVar::ItemFuelSupply {
            category: vec!["chemical".to_string(), "biological".to_string()],
            has_burnt_result: true,
        }],
        2_000_000.0
    );
    assert_eq!(flow.len(), 3, "1 消耗 + 1 燃尽产物 + 1 燃料供给流");
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
fn tile_extract_produces_the_tiles_fluid() {
    // **地格决定产出什么流体**（原版 offshore-pump 的 fluid_box 没有 filter），
    // 机械只决定速率：pumping_speed 20/刻 × 60 = 1200/s，温度 = 流体默认温度。
    let flow = flow(
        json!({
            "tile": { "water": { "fluid": "water" } },
            "fluid": { "water": { "default_temperature": 15.0, "max_temperature": 100.0 } },
            "offshore-pump": { "offshore-pump": { "pumping_speed": 20.0 } }
        }),
        Mechanic::TileExtract(TileExtractMechanic {
            tile: "water".to_string(),
            machine: id("offshore-pump"),
        }),
    );

    assert_eq!(
        flow[&DualVar::Fluid {
            name: "water".to_string(),
            temperature: [15, 15],
        }],
        1200.0
    );
}

#[test]
fn tile_dispose_consumes_the_item_without_output() {
    // 只有带 destroys_dropped_items 的地格能销毁（原版岩浆）。
    // 消耗 1 单位/秒，**没有产出**——成本由 instance_cost 给（1/16 格 每（个/秒））。
    let flow = flow(
        json!({
            "tile": { "lava": { "destroys_dropped_items": true } },
            "item": { "stone": { "stack_size": 50 } }
        }),
        Mechanic::TileDispose(TileDisposeMechanic {
            tile: "lava".to_string(),
            item: id("stone"),
        }),
    );

    assert_eq!(flow[&DualVar::Item(id("stone"))], -1.0);
    assert_eq!(flow.len(), 1, "销毁只消耗物品、没有产出: {flow:?}");
}

#[test]
fn tile_dispose_refuses_a_tile_that_cannot_destroy_items() {
    // 没有 destroys_dropped_items 的地格 → **不生成任何变量**，而不是偷偷允许。
    // 判据来自原型数据，不是我们规定的。
    let dump = json!({
        "tile": { "grass": {} },
        "item": { "stone": { "stack_size": 50 } }
    });
    let store = PrototypeStore::load(&dump).expect("dump should load");
    let game = GameState {
        max_quality: store.quality_order().len().saturating_sub(1),
        ..Default::default()
    };
    let ctx = Context::new(&store, &game);
    let mechanic = Mechanic::TileDispose(TileDisposeMechanic {
        tile: "grass".to_string(),
        item: id("stone"),
    });
    let expansion = expand([(0usize, &mechanic)], &ctx);
    assert_eq!(
        expansion.len(),
        0,
        "不能销毁物品的地格不应产生任何变量"
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
            output_temperature: None,
            fuel: None,
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

/// heat-fluid-inside 模式：原型不换流体，把**同一种流体**从输入温度加热到
/// 指定的输出温度（连续加热，输出温度任选）。流量由功率 / (比热容 × 温差) 决定。
#[test]
fn boiler_heat_fluid_inside_raises_same_fluid_temperature() {
    let flow = flow(
        json!({
            "fluid": {
                "water": { "default_temperature": 15.0, "max_temperature": 100.0, "heat_capacity": "1kJ" }
            },
            "boiler": {
                "fluid-heater": {
                    "mode": "heat-fluid-inside",
                    "energy_consumption": "1MW",
                    "energy_source": { "type": "electric" },
                    "fluid_box": { "production_type": "input-output" }
                }
            }
        }),
        Mechanic::Boiler(BoilerMechanic {
            boiler: id("fluid-heater"),
            fluid: "water".to_string(),
            temperature: Some(15),
            output_temperature: Some(100),
            fuel: None,
        }),
    );

    let cold = flow[&DualVar::Fluid {
        name: "water".to_string(),
        temperature: [15, 15],
    }];
    let hot = flow[&DualVar::Fluid {
        name: "water".to_string(),
        temperature: [100, 100],
    }];
    assert!(cold < 0.0, "输入温度那一档应被消耗：{cold}");
    assert!(hot > 0.0, "输出温度那一档应被产出：{hot}");
    assert!(
        (cold + hot).abs() < 1e-9,
        "同流体升温不改变总量：{cold} vs {hot}"
    );
    // 1MW、比热容 1kJ/单位/℃、温差 85℃ ⇒ 1e6 / 1000 / 85 单位/秒
    assert!(
        (hot - 1_000_000.0 / 1000.0 / 85.0).abs() < 1e-6,
        "流量应由功率/比热容/温差决定：{hot}"
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
            fuel: Some(Fuel::Item { item: id("fuel") }),
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
            fuel: Some(Fuel::Item {
                item: IdWithQuality::new("fuel", "quality"),
            }),
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
