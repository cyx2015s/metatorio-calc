use metatorio_core::{
    Accessibility, AccessibilityOptions, Accessible, BeaconConfig, Context, GameState,
    IdWithQuality, Mechanic, ModuleConfig,
};
use metatorio_data::store::PrototypeStore;
use metatorio_runtime::{
    auto_plan::{EnumerateOptions, enumerate_all},
    *,
};
use serde_json::{Value, json};

fn fixture() -> Value {
    let mut dump = json!({
        "item": {
            "ore": {}, "gear": {}, "red": {},
            "seed": {"plant_result": "plant", "default_import_location": "nauvis"},
            "food": {"spoil_ticks": 60, "spoil_result": "ore"},
            "coal": {"fuel_value": "1MJ", "fuel_categories": ["chemical"]},
            "satellite": {"rocket_launch_products": [{"name": "red", "amount": 1}]}
        },
        "fluid": {"water": {"default_temperature": 15}, "steam": {"default_temperature": 15, "max_temperature": 500}},
        "quality": {"normal": {"level": 0, "order": "a", "next": "legendary"}, "legendary": {"level": 5, "order": "b"}},
        "lab": {"lab": {"inputs": ["red"]}},
        "plant": {"plant": {"growth_ticks": 60, "surface_conditions": [{"property": "gravity", "min": 1}]}},
        "tile": {"water": {"fluid": "water"}, "lava": {"destroys_dropped_items": true}},
        "planet": {
            "nauvis": {"surface_properties": {"gravity": 10, "pressure": 1000}, "map_gen_settings": {
                "autoplace_settings": {"tile": {"settings": {"water": {}, "lava": {}}}}
            }},
            "airless": {"surface_properties": {"gravity": 0, "pressure": 0}, "map_gen_settings": {
                "autoplace_settings": {"tile": {"settings": {"water": {}, "lava": {}}}}
            }}
        },
        "surface": {"space-platform": {"surface_properties": {"gravity": 0, "pressure": 0}}},
        "offshore-pump": {"pump": {"pumping_speed": 1, "energy_source": {"type": "void"}}},
        "resource": {"ore-node": {"minable": {"mining_time": 1, "result": "ore"}}},
        "recipe": {"gear": {"enabled": true, "energy_required": 1, "ingredients": [{"type": "item", "name": "ore", "amount": 1}], "results": [{"type": "item", "name": "gear", "amount": 1}]}},
        "beacon": {"beacon": {"distribution_effectivity": 1, "module_slots": 2}},
        "rocket-silo": {"silo": {"surface_conditions": [{"property": "pressure", "min": 1}]}}
    });
    for (name, speed) in [("fast", 100), ("slow", 1)] {
        dump["assembling-machine"][name] = json!({"crafting_speed": speed, "crafting_categories": ["crafting"], "energy_usage": "0J", "energy_source": {"type": "void"}});
        dump["mining-drill"][format!("{name}-drill")] = json!({"mining_speed": speed, "resource_categories": ["basic-solid"], "energy_usage": "0J", "energy_source": {"type": "void"}});
    }
    for (name, condition) in [
        ("ground", json!({"property": "pressure", "min": 1})),
        ("space", json!({"property": "pressure", "max": 0})),
    ] {
        let conditions = json!([condition]);
        dump["generator"][format!("{name}-generator")] = json!({"surface_conditions": conditions, "fluid_box": {"filter": "steam"}, "maximum_temperature": 500, "fluid_usage_per_tick": 1});
        dump["boiler"][format!("{name}-boiler")] = json!({"surface_conditions": conditions, "mode": "output-to-separate-pipe", "target_temperature": 165, "fluid_box": {"filter": "water"}, "output_fluid_box": {"filter": "steam"}});
        dump["reactor"][format!("{name}-reactor")] = json!({"surface_conditions": conditions});
        dump["solar-panel"][format!("{name}-panel")] =
            json!({"surface_conditions": conditions, "production": "60kW"});
        dump["accumulator"][format!("{name}-accumulator")] = json!({"surface_conditions": conditions, "energy_source": {"type": "electric", "buffer_capacity": "5MJ"}});
    }
    dump
}

fn options(planet: Option<&str>, surface: Option<&str>) -> EnumerateOptions {
    EnumerateOptions {
        alternative_count: 1,
        machine_preferences: vec![],
        enumerate_modules: vec![],
        enumerate_beacons: vec![],
        quality_limit: 0,
        major_quality: 0,
        planet: planet.map(str::to_owned),
        surface: surface.map(str::to_owned),
        accessibility: None,
    }
}

fn enumerate(store: &PrototypeStore, options: &EnumerateOptions) -> Vec<Mechanic> {
    let game = GameState::default();
    enumerate_all(store, &Context::new(store, &game), options)
}

#[test]
fn platform_does_not_inherit_ground_tiles_mines_plants_or_silos() {
    let store = PrototypeStore::load(&fixture()).unwrap();
    let ground = enumerate(&store, &options(Some("nauvis"), None));
    for predicate in [
        (|m: &Mechanic| matches!(m, Mechanic::TileExtract(_))) as fn(&Mechanic) -> bool,
        |m| matches!(m, Mechanic::TileDispose(_)),
        |m| matches!(m, Mechanic::Mining(_)),
        |m| matches!(m, Mechanic::Plant(_)),
        |m| matches!(m, Mechanic::ItemLaunch(_)),
    ] {
        assert!(ground.iter().any(predicate));
    }
    for planet in [Some("nauvis"), None] {
        let space = enumerate(&store, &options(planet, Some("space-platform")));
        assert!(!space.iter().any(|m| matches!(
            m,
            Mechanic::TileExtract(_)
                | Mechanic::TileDispose(_)
                | Mechanic::Mining(_)
                | Mechanic::Plant(_)
                | Mechanic::ItemLaunch(_)
        )));
        assert!(space.iter().any(|m| matches!(m, Mechanic::Spoil(_))));
        assert!(space.iter().any(|m| matches!(m, Mechanic::ItemFuel(_))));
        assert!(
            space
                .iter()
                .any(|m| matches!(m, Mechanic::VirtualScience(_)))
        );
    }
    // 零重力的普通星球仍有地面，不能用 gravity/pressure=0 一刀切。
    let airless = enumerate(&store, &options(Some("airless"), None));
    assert!(
        airless
            .iter()
            .any(|m| matches!(m, Mechanic::TileExtract(_)))
    );
    assert!(airless.iter().any(|m| matches!(m, Mechanic::Mining(_))));
    assert!(!airless.iter().any(|m| matches!(m, Mechanic::Plant(_))));
}

#[test]
fn accessible_machines_are_selected_before_truncation_and_locked_fallback_is_removed() {
    let store = PrototypeStore::load(&fixture()).unwrap();
    let mut opts = options(Some("nauvis"), None);
    opts.machine_preferences = vec![
        IdWithQuality::new("fast", "normal"),
        IdWithQuality::new("fast-drill", "normal"),
    ];
    opts.accessibility = Some(metatorio_core::compute_accessibility(
        &store,
        &AccessibilityOptions {
            forced_accessible: ["slow", "slow-drill", "pump"]
                .into_iter()
                .map(|name| Accessible::Entity(name.into()))
                .collect(),
            ..Default::default()
        },
    ));
    opts.enumerate_beacons = vec![AutoBeaconPlan {
        module_config: ModuleConfig {
            beacons: vec![BeaconConfig {
                beacon: IdWithQuality::new("beacon", "normal"),
                ..Default::default()
            }],
            ..Default::default()
        },
    }];
    let candidates = enumerate(&store, &opts);
    let recipes: Vec<_> = candidates
        .iter()
        .filter_map(|m| match m {
            Mechanic::Recipe(m) => Some(m),
            _ => None,
        })
        .collect();
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0].machine.id, "slow");
    assert!(
        recipes[0].module_config.beacons.is_empty(),
        "未解锁的塔不应绕过可达性"
    );
    assert!(
        candidates
            .iter()
            .any(|m| matches!(m, Mechanic::Mining(m) if m.machine.id == "slow-drill"))
    );
    opts.accessibility = Some(Accessibility::default());
    assert!(!enumerate(&store, &opts).iter().any(|m| matches!(
        m,
        Mechanic::Recipe(_) | Mechanic::Mining(_) | Mechanic::TileExtract(_)
    )));
}

#[test]
fn energy_equipment_and_launch_infrastructure_obey_surface_and_accessibility() {
    let store = PrototypeStore::load(&fixture()).unwrap();
    let opts = options(Some("nauvis"), Some("space-platform"));
    let candidates = enumerate(&store, &opts);
    let mut checked = std::collections::BTreeSet::new();
    for candidate in candidates {
        let (kind, names) = match candidate {
            Mechanic::Generator(m) => ("generator", vec![m.generator.id]),
            Mechanic::Boiler(m) => ("boiler", vec![m.boiler.id]),
            Mechanic::Reactor(m) => ("reactor", vec![m.reactor.id]),
            Mechanic::Solar(m) => {
                assert!(m.accumulator.id.is_empty(), "轨道不应要求蓄电器");
                ("solar", vec![m.solar_panel.id])
            }
            _ => continue,
        };
        checked.insert(kind);
        assert!(
            names.iter().all(|name| name.starts_with("space-")),
            "{names:?}"
        );
    }
    assert_eq!(
        checked,
        ["generator", "boiler", "reactor", "solar"]
            .into_iter()
            .collect()
    );
    let mut dump = fixture();
    dump["rocket-silo"]["silo"]["surface_conditions"] = json!([{"property": "pressure", "max": 0}]);
    let modded = PrototypeStore::load(&dump).unwrap();
    assert!(
        enumerate(&modded, &opts)
            .iter()
            .any(|m| matches!(m, Mechanic::ItemLaunch(_))),
        "发射井原型允许时保留模组平台发射能力"
    );
    let mut locked = opts;
    locked.accessibility = Some(Accessibility::default());
    assert!(!enumerate(&modded, &locked).iter().any(|m| matches!(
        m,
        Mechanic::ItemLaunch(_)
            | Mechanic::Generator(_)
            | Mechanic::Boiler(_)
            | Mechanic::Reactor(_)
            | Mechanic::Solar(_)
    )));
}

#[test]
fn mining_candidates_are_not_duplicated_by_product_quality_limit() {
    let store = PrototypeStore::load(&fixture()).unwrap();
    let mut opts = options(Some("nauvis"), None);
    let count = |opts: &EnumerateOptions| {
        enumerate(&store, opts)
            .iter()
            .filter(|m| matches!(m, Mechanic::Mining(_)))
            .count()
    };
    assert_eq!(count(&opts), 1);
    opts.quality_limit = 1;
    assert_eq!(count(&opts), 1);
}

#[test]
fn resources_and_plants_obey_their_own_surface_conditions() {
    let mut dump = fixture();
    dump["resource"]["ore-node"]["surface_conditions"] =
        json!([{"property": "pressure", "min": 1}]);
    let store = PrototypeStore::load(&dump).unwrap();
    let airless = enumerate(&store, &options(Some("airless"), None));
    assert!(
        !airless
            .iter()
            .any(|m| matches!(m, Mechanic::Mining(_) | Mechanic::Plant(_)))
    );
    let ground = enumerate(&store, &options(Some("nauvis"), None));
    assert!(ground.iter().any(|m| matches!(m, Mechanic::Mining(_))));
    assert!(ground.iter().any(|m| matches!(m, Mechanic::Plant(_))));
}

#[test]
fn vanilla_planets_keep_their_ground_resources_out_of_orbit_candidates() {
    let dump = serde_json::from_str(include_str!("../../../assets/data-raw-dump.json")).unwrap();
    let store = PrototypeStore::load(&dump).unwrap();
    for planet in ["nauvis", "vulcanus", "gleba"] {
        let space = enumerate(&store, &options(Some(planet), Some("space-platform")));
        assert!(
            !space.iter().any(|m| matches!(
                m,
                Mechanic::TileExtract(_)
                    | Mechanic::TileDispose(_)
                    | Mechanic::Mining(_)
                    | Mechanic::Plant(_)
                    | Mechanic::ItemLaunch(_)
            )),
            "{planet}"
        );
        assert!(
            space
                .iter()
                .any(|m| matches!(m, Mechanic::Recipe(m) if m.machine.id == "crusher"))
        );
        assert!(space.iter().any(|m| matches!(m, Mechanic::Solar(_))));
    }
}

#[test]
fn planning_cannot_pump_planet_water_in_orbit_but_preserves_explicit_manual_exceptions() {
    let mut runtime = Runtime::new();
    runtime.install_context("test".into(), PrototypeStore::load(&fixture()).unwrap());
    runtime.set_active_context(Some("test".into()));
    runtime
        .dispatch(AppMessage::Application(ApplicationAction::NewProject {
            name: "test".into(),
        }))
        .unwrap();
    let project = runtime.state.document.projects[0].id;
    runtime
        .dispatch(AppMessage::Project {
            project,
            action: ProjectAction::SetAllAccessible { enabled: true },
        })
        .unwrap();
    runtime
        .dispatch(AppMessage::Project {
            project,
            action: ProjectAction::AddFactory {
                name: "test".into(),
                template: FactoryTemplate::Empty,
            },
        })
        .unwrap();
    let factory = runtime.state.project(project).unwrap().factories[0].id;
    runtime
        .dispatch(AppMessage::Factory {
            project,
            factory,
            action: FactoryAction::Flow(FlowAction::AddToTarget {
                flow: metatorio_core::DualVar::Fluid {
                    name: "water".into(),
                    temperature: [15, 15],
                },
                amount: 1.0,
            }),
        })
        .unwrap();
    assert!(matches!(
        runtime.auto_plan(project, factory).unwrap().status,
        SolveStatus::Solved { .. }
    ));
    runtime
        .dispatch(AppMessage::Factory {
            project,
            factory,
            action: FactoryAction::Context(FactoryContextAction::SetSurface {
                surface: Some("space-platform".into()),
            }),
        })
        .unwrap();
    assert!(
        runtime.auto_plan(project, factory).is_err(),
        "不能把下方星球的水凭空搬上平台"
    );
    runtime
        .dispatch(AppMessage::Project {
            project,
            action: ProjectAction::Planning(PlanningAction::SetPreserveExistingMechanics {
                preserve: true,
            }),
        })
        .unwrap();
    assert!(
        matches!(
            runtime.auto_plan(project, factory).unwrap().status,
            SolveStatus::Solved { .. }
        ),
        "保留手动机制是显式环境例外"
    );
}

#[test]
fn tile_reachability_recurses_on_ground_and_platform_without_crossing_environments() {
    let mut dump = fixture();
    dump["tile"]["empty-space"] = json!({"collision_mask": {"layers": {"empty_space": true}}});
    dump["tile"]["deck"] = json!({"collision_mask": {"layers": {"floor": true}}});
    dump["tile"]["pool"] = json!({"fluid": "water"});
    dump["tile"]["chute"] = json!({"destroys_dropped_items": true});
    dump["item"]["deck"] = json!({"place_as_tile": {"result": "deck", "condition": {"layers": {"empty_space": true}}, "invert": true}});
    dump["item"]["pool"] = json!({"place_as_tile": {"result": "pool", "condition": {"layers": {}}, "tile_condition": ["deck"]}});
    dump["item"]["chute"] = json!({"place_as_tile": {"result": "chute", "condition": {"layers": {}}, "tile_condition": ["pool"]}});
    // A disconnected cycle must never bootstrap itself.
    for (name, parent) in [("cycle-a", "cycle-b"), ("cycle-b", "cycle-a")] {
        dump["tile"][name] = json!({"fluid": "water"});
        dump["item"][name] = json!({"place_as_tile": {"result": name, "condition": {"layers": {}}, "tile_condition": [parent]}});
    }
    let store = PrototypeStore::load(&dump).unwrap();
    let available =
        |planet, surface| metatorio_core::planet::surface_available_tiles(&store, planet, surface);
    for planet in [None, Some("nauvis"), Some("airless")] {
        let tiles = available(planet, Some("space-platform"));
        assert_eq!(
            tiles,
            ["empty-space", "deck", "pool", "chute"]
                .map(str::to_owned)
                .into_iter()
                .collect()
        );
        let mechanics = enumerate(&store, &options(planet, Some("space-platform")));
        assert!(
            mechanics
                .iter()
                .any(|m| matches!(m, Mechanic::TileExtract(m) if m.tile == "pool"))
        );
        assert!(
            mechanics
                .iter()
                .any(|m| matches!(m, Mechanic::TileDispose(m) if m.tile == "chute"))
        );
    }
    assert_eq!(
        available(Some("nauvis"), None),
        ["water", "lava"].map(str::to_owned).into_iter().collect()
    );
    // The identical chain is also reachable from a natural ground tile.
    dump["item"]["deck"]["place_as_tile"] =
        json!({"result": "deck", "condition": {"layers": {}}, "tile_condition": ["water"]});
    let ground_store = PrototypeStore::load(&dump).unwrap();
    let ground =
        metatorio_core::planet::surface_available_tiles(&ground_store, Some("nauvis"), None);
    assert!(ground.contains("chute"));
    assert!(!ground.contains("cycle-a"));
    // A non-coverable intermediate tile stops subsequent placement.
    dump["tile"]["deck"]["allows_being_covered"] = json!(false);
    let blocked = PrototypeStore::load(&dump).unwrap();
    let tiles = metatorio_core::planet::surface_available_tiles(&blocked, Some("nauvis"), None);
    assert!(tiles.contains("deck"));
    assert!(!tiles.contains("pool"));
}

#[test]
fn platform_starter_tiles_are_roots_and_do_not_require_an_orbit_planet() {
    let mut dump = fixture();
    dump["space-platform-starter-pack"] = json!({
        "starter": {"surface": "space-platform", "tiles": [{"tile": "seed-deck", "position": [0,0]}]},
        "other": {"surface": "other-platform", "tiles": [{"tile": "water", "position": [0,0]}]}
    });
    dump["tile"]["seed-deck"] = json!({});
    dump["item"]["chute"] = json!({"place_as_tile": {"result": "lava", "condition": {"layers": {}}, "tile_condition": ["seed-deck"]}});
    let store = PrototypeStore::load(&dump).unwrap();
    let tiles =
        metatorio_core::planet::surface_available_tiles(&store, None, Some("space-platform"));
    assert_eq!(
        tiles,
        ["seed-deck", "lava"]
            .map(str::to_owned)
            .into_iter()
            .collect()
    );
    assert!(
        enumerate(&store, &options(None, Some("space-platform")))
            .iter()
            .any(|m| matches!(m, Mechanic::TileDispose(_)))
    );
}

#[test]
fn vanilla_tile_closure_reaches_foundations_without_importing_planet_terrain() {
    let dump = serde_json::from_str(include_str!("../../../assets/data-raw-dump.json")).unwrap();
    let store = PrototypeStore::load(&dump).unwrap();
    let space = metatorio_core::planet::surface_available_tiles(
        &store,
        Some("vulcanus"),
        Some("space-platform"),
    );
    assert!(space.contains("empty-space"));
    assert!(space.contains("space-platform-foundation"));
    assert!(!space.contains("lava"));
    assert!(!space.contains("water"));
    let ground = metatorio_core::planet::surface_available_tiles(&store, Some("vulcanus"), None);
    assert!(ground.contains("foundation"));
    assert!(ground.contains("concrete"));
    assert!(!ground.contains("space-platform-foundation"));
}
