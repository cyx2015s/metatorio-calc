use metatorio_core::expand::expand;
use metatorio_core::*;
use metatorio_data::store::PrototypeStore;
use metatorio_runtime::solve::apply_environment_to_game_state;
use serde_json::json;

fn fixture() -> PrototypeStore {
    PrototypeStore::load(&json!({
        "planet": {
            "cold":{"entities_require_heating":true},
            "warm":{"entities_require_heating":false}, "default":{}
        },
        "space-location":{"deep":{}},
        "quality":{"normal":{"level":0,"order":"a","next":"rare"},"rare":{"level":2,"order":"b"}},
        "item":{
            "ore":{}, "product":{},
            "seed":{"plant_result":"tree"},
            "food":{"spoil_ticks":60,"spoil_result":"ore"},
            "fuel":{"fuel_value":"1MJ","fuel_categories":["chemical"]},
            "science":{},
            "satellite":{"stack_size":1,"rocket_launch_products":[{"name":"science","amount":1}]}
        },
        "lab":{"lab":{"inputs":["science"],"heating_energy":"1MW"}},
        "fluid":{"water":{"default_temperature":15,"max_temperature":500,"heat_capacity":"200J","fuel_value":"1MJ"}},
        "recipe":{"product":{"energy_required":1,"ingredients":[],"results":[{"type":"item","name":"product","amount":1}]}},
        "assembling-machine":{
            "machine":{"crafting_speed":1,"energy_usage":"50kW","energy_source":{"type":"heat"},"heating_energy":"2kJ"},
            "unheated":{"crafting_speed":1,"energy_source":{"type":"void"}}
        },
        "beacon":{"beacon":{"distribution_effectivity":1,"energy_usage":"480kW","energy_source":{"type":"electric"},"heating_energy":"100kW"}},
        "module":{"module":{"effect":{"speed":1,"consumption":-0.8}}},
        "resource":{"node":{"minable":{"mining_time":1,"result":"ore"}}},
        "mining-drill":{"drill":{"mining_speed":1,"energy_source":{"type":"void"},"heating_energy":"200kW"}},
        "tile":{"liquid":{"fluid":"water"},"sink":{"destroys_dropped_items":true}},
        "offshore-pump":{"pump":{"pumping_speed":1,"heating_energy":"300kW"}},
        "plant":{"tree":{"growth_ticks":60,"minable":{"mining_time":1,"result":"ore"},"heating_energy":"400kW"}},
        "generator":{"generator":{"fluid_box":{"filter":"water"},"fluid_usage_per_tick":1,"maximum_temperature":500,"heating_energy":"500kW"}},
        "boiler":{"boiler":{"energy_source":{"type":"void"},"energy_consumption":"1MW","mode":"heat-fluid-inside","fluid_box":{"filter":"water"},"heating_energy":"600kW"}},
        "reactor":{"reactor":{"energy_source":{"type":"void"},"consumption":"10MW","heating_energy":"700kW"}},
        "solar-panel":{"panel":{"production":"60kW","heating_energy":"800kW"}},
        "accumulator":{"storage":{"energy_source":{"type":"electric","buffer_capacity":"5MJ"},"heating_energy":"900kW"}},
        "rocket-silo":{"silo":{"crafting_speed":1,"energy_source":{"type":"void"},"rocket_parts_required":50,"to_be_inserted_to_rocket_inventory_size":1,"heating_energy":"1MW"}}
    })).unwrap()
}

fn environment(store: &PrototypeStore, planet: Option<&str>, surface: Option<&str>) -> GameState {
    let mut state = GameState {
        qualities: vec!["normal".into(), "rare".into()],
        max_quality: 1,
        ..Default::default()
    };
    apply_environment_to_game_state(store, &mut state, planet, surface);
    state
}

fn heat(store: &PrototypeStore, state: &GameState, mechanic: &Mechanic) -> f64 {
    let expansion = expand([(0, mechanic)], &Context::new(store, state));
    assert!(!expansion.is_empty(), "{mechanic:?} must actually expand");
    let first = expansion.variables[0]
        .flow
        .get(&DualVar::Heat)
        .copied()
        .unwrap_or(0.0);
    for variable in &expansion.variables {
        near(
            variable.flow.get(&DualVar::Heat).copied().unwrap_or(0.0),
            first,
        );
    }
    first
}

fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

fn recipe(machine: &str) -> Mechanic {
    Mechanic::Recipe(RecipeMechanic {
        recipe: "product".into(),
        machine: machine.into(),
        ..Default::default()
    })
}

#[test]
fn environment_flag_comes_from_planet_and_resets_on_every_switch() {
    let store = fixture();
    let mut state = GameState::default();
    for (planet, surface, expected) in [
        (Some("cold"), None, true),
        (Some("cold"), Some("space-platform"), false),
        (Some("cold"), None, true),
        (Some("warm"), None, false),
        (Some("cold"), Some("custom-orbit"), false),
        (Some("default"), None, false),
        (Some("deep"), None, false),
        (Some("missing"), None, false),
        (None, None, false),
    ] {
        apply_environment_to_game_state(&store, &mut state, planet, surface);
        assert_eq!(
            state.entities_require_heating, expected,
            "{planet:?}/{surface:?}"
        );
    }
}

#[test]
fn all_stationary_mechanisms_add_heating_without_scaling_existing_flows() {
    let store = fixture();
    let cold = environment(&store, Some("cold"), None);
    let warm = environment(&store, Some("warm"), None);
    let cases = [
        (recipe("machine"), 120_000.),
        (
            Mechanic::Mining(MiningMechanic {
                resource: "node".into(),
                machine: "drill".into(),
                ..Default::default()
            }),
            200_000.,
        ),
        (
            Mechanic::TileExtract(TileExtractMechanic {
                tile: "liquid".into(),
                machine: "pump".into(),
            }),
            300_000.,
        ),
        (
            Mechanic::Plant(PlantMechanic {
                seed: "seed".into(),
            }),
            400_000.,
        ),
        (
            Mechanic::Generator(GeneratorMechanic {
                generator: "generator".into(),
                fluid: "water".into(),
                temperature: Some(165),
            }),
            500_000.,
        ),
        (
            Mechanic::Boiler(BoilerMechanic {
                boiler: "boiler".into(),
                fluid: "water".into(),
                output_temperature: Some(165),
                ..Default::default()
            }),
            600_000.,
        ),
        (
            Mechanic::Reactor(ReactorMechanic {
                reactor: "reactor".into(),
                ..Default::default()
            }),
            700_000.,
        ),
        (recipe("silo"), 1_000_000.),
    ];
    for (mechanic, heating) in cases {
        near(
            heat(&store, &cold, &mechanic),
            heat(&store, &warm, &mechanic) - heating,
        );
        let a = expand([(0, &mechanic)], &Context::new(&store, &cold));
        let b = expand([(0, &mechanic)], &Context::new(&store, &warm));
        assert_eq!(a.variables.len(), b.variables.len());
        for (a, b) in a.variables.iter().zip(&b.variables) {
            near(a.cost, b.cost);
            let mut a = a.flow.clone();
            a.shift_remove(&DualVar::Heat);
            let mut b = b.flow.clone();
            b.shift_remove(&DualVar::Heat);
            assert_eq!(a, b, "{mechanic:?}");
        }
    }
    near(heat(&store, &cold, &recipe("machine")), -170_000.); // 工作热耗 + 供暖
    near(heat(&store, &cold, &cases_reactor()), 9_300_000.); // 产热净额，不能覆盖产出
}

fn cases_reactor() -> Mechanic {
    Mechanic::Reactor(ReactorMechanic {
        reactor: "reactor".into(),
        ..Default::default()
    })
}

#[test]
fn heating_counts_shared_beacons_but_ignores_speed_efficiency_and_quality_bonuses() {
    let store = fixture();
    let state = environment(&store, Some("cold"), None);
    let Mechanic::Recipe(mut r) = recipe("machine") else {
        unreachable!()
    };
    r.machine.quality = "rare".into();
    r.module_config.modules = vec!["module".into()];
    r.module_config.beacons = vec![BeaconConfig {
        beacon: IdWithQuality::new("beacon", "rare"),
        count: 8,
        share: 8.,
        modules: vec![],
    }];
    // 机器供暖120kW + 8/8座信标100kW；工作热耗另受节能插件影响。
    let cold = heat(&store, &state, &Mechanic::Recipe(r.clone()));
    let mut warm = state.clone();
    warm.entities_require_heating = false;
    near(
        cold - heat(&store, &warm, &Mechanic::Recipe(r.clone())),
        -220_000.,
    );
    r.module_config.beacons[0].count = 0;
    near(
        heat(&store, &state, &Mechanic::Recipe(r.clone()))
            - heat(&store, &warm, &Mechanic::Recipe(r)),
        -120_000.,
    );
    let mining = Mechanic::Mining(MiningMechanic {
        resource: "node".into(),
        machine: IdWithQuality::new("drill", "rare"),
        module_config: ModuleConfig {
            modules: vec![],
            beacons: vec![BeaconConfig {
                beacon: "beacon".into(),
                count: 8,
                share: 4.,
                modules: vec![],
            }],
        },
        fuel: None,
    });
    near(heat(&store, &state, &mining), -400_000.);
}

#[test]
fn solar_heats_the_actual_fractional_bank_and_respects_storage_quality() {
    let store = fixture();
    let state = environment(&store, Some("cold"), None);
    for quality in ["normal", "rare"] {
        let solar = SolarMechanic {
            solar_panel: "panel".into(),
            accumulator: IdWithQuality::new("storage", quality),
        };
        let balance = solar_balance(&Context::new(&store, &state), &solar).unwrap();
        let expected = -800_000. - 900_000. * balance.recommended_accumulators;
        near(heat(&store, &state, &Mechanic::Solar(solar)), expected);
    }
    let orbit = environment(&store, Some("cold"), Some("space-platform"));
    near(
        heat(
            &store,
            &orbit,
            &Mechanic::Solar(SolarMechanic {
                solar_panel: "panel".into(),
                accumulator: "missing".into(),
            }),
        ),
        0.,
    );
}

#[test]
fn conversions_and_entities_without_heating_do_not_invent_buildings() {
    let store = fixture();
    let state = environment(&store, Some("cold"), None);
    for mechanic in [
        recipe("unheated"),
        Mechanic::ItemLaunch(ItemLaunchMechanic {
            item: "satellite".into(),
            weight_mode: false,
        }),
        Mechanic::ItemFuel(ItemFuelMechanic {
            item: "fuel".into(),
        }),
        Mechanic::FluidFuel(FluidFuelMechanic {
            fluid: "water".into(),
        }),
        Mechanic::FluidHeat(FluidHeatMechanic {
            fluid: "water".into(),
            temperature: Some(165),
        }),
        Mechanic::Spoil(SpoilMechanic {
            item: "food".into(),
        }),
        Mechanic::TileDispose(TileDisposeMechanic {
            tile: "sink".into(),
            item: "ore".into(),
        }),
        Mechanic::VirtualScience(VirtualScienceMechanic {
            item: "science".into(),
        }),
    ] {
        near(heat(&store, &state, &mechanic), 0.);
    }
    assert!(expand([(0, &recipe("missing"))], &Context::new(&store, &state)).is_empty());
}

#[test]
fn real_aquilo_consumes_heating_but_its_orbit_and_nauvis_do_not() {
    let store = PrototypeStore::load(
        &serde_json::from_slice(
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/data-raw-dump.json"
            ))
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let mechanic = Mechanic::Recipe(RecipeMechanic {
        recipe: "iron-gear-wheel".into(),
        machine: "assembling-machine-2".into(),
        ..Default::default()
    });
    for (planet, surface, expected) in [
        ("aquilo", None, -100_000.),
        ("aquilo", Some("space-platform"), 0.),
        ("nauvis", None, 0.),
    ] {
        near(
            heat(
                &store,
                &environment(&store, Some(planet), surface),
                &mechanic,
            ),
            expected,
        );
    }
}

#[test]
fn runtime_solver_balances_heating_against_net_reactor_output() {
    use metatorio_runtime::{
        FactoryDocument, FactoryId, FactorySettings, FlowTarget, MechanicEntry, MechanicId,
        ProjectDocument, ProjectId, ProjectSettings, Runtime, SolveStatus,
    };
    for (planet, surface, required_heat, reactor_heat) in [
        ("cold", None, 170_000., 9_300_000.),
        ("warm", None, 50_000., 10_000_000.),
        ("cold", Some("space-platform"), 50_000., 10_000_000.),
    ] {
        let mut runtime = Runtime::new();
        runtime.install_context("test".into(), fixture());
        runtime.set_active_context(Some("test".into()));
        runtime.state.document.projects.push(ProjectDocument {
            id: ProjectId(1),
            settings: ProjectSettings {
                all_accessible: true,
                ..Default::default()
            },
            factories: vec![FactoryDocument {
                id: FactoryId(1),
                strict_source: true,
                settings: FactorySettings {
                    planet: Some(planet.into()),
                    surface: surface.map(str::to_owned),
                    ..Default::default()
                },
                targets: vec![FlowTarget {
                    flow: DualVar::Item("product".into()),
                    amount: 1.,
                    ..Default::default()
                }],
                mechanics: vec![
                    MechanicEntry {
                        id: MechanicId(1),
                        enabled: true,
                        mechanic: recipe("machine"),
                    },
                    MechanicEntry {
                        id: MechanicId(2),
                        enabled: true,
                        mechanic: cases_reactor(),
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        });
        let result = runtime.solve_factory(ProjectId(1), FactoryId(1)).unwrap();
        let SolveStatus::Solved { cost, .. } = result.status else {
            panic!("{result:?}")
        };
        near(cost, 16. * (1. + required_heat / reactor_heat));
    }
}
