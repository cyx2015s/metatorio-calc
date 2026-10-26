use metatorio_core::{expand::expand, *};
use metatorio_data::store::PrototypeStore;
use serde_json::{Value, json};
fn dump() -> Value {
    json!({
     "quality":{"normal":{"level":0,"next":"rare","order":"a"},"rare":{"level":2,"default_multiplier":1.6,"order":"b"}},
     "fluid":{"cold":{"default_temperature":-150,"max_temperature":-150},"hot":{"default_temperature":180},"plasma":{"default_temperature":1000000,"max_temperature":10000000,"heat_capacity":"25J","fuel_value":"100MJ"}},
     "item":{"cell":{"fuel_value":"40GJ","fuel_categories":["fusion"]},"coal":{"fuel_value":"4MJ","fuel_categories":["chemical"]},"junk":{}},
     "fusion-reactor":{"r":{"max_fluid_usage":4.0/60.0,"power_input":"10MW","heating_energy":"1MW","collision_box":[[-3,-3],[3,3]],
        "burner":{"fuel_categories":["fusion"],"effectivity":1},"energy_source":{"type":"electric","usage_priority":"primary-input"},
        "input_fluid_box":{"filter":"cold"},"output_fluid_box":{"filter":"plasma"}}},
     "fusion-generator":{"g":{"max_fluid_usage":2.0/60.0,"heating_energy":"2MW","collision_box":[[-1.5,-2.5],[1.5,2.5]],
        "input_fluid_box":{"filter":"plasma"},"output_fluid_box":{"filter":"hot"},"energy_source":{"type":"electric","usage_priority":"secondary-output","output_flow_limit":"50MW"}}},
     "reactor":{"n":{"consumption":"40MW","neighbour_bonus":1,"energy_source":{"type":"burner","fuel_categories":["fusion"]},"heat_buffer":{"max_transfer":"10GW","specific_heat":"10MJ","max_temperature":1000}}}
    })
}
fn id(s: &str) -> IdWithQuality {
    IdWithQuality::new(s, "normal")
}
fn reactor(neighbours: Option<u8>) -> Mechanic {
    Mechanic::FusionReactor(FusionReactorMechanic {
        reactor: id("r"),
        neighbours,
        fuel: Some(Fuel::Item { item: id("cell") }),
    })
}
fn generator(temperature: i32) -> Mechanic {
    Mechanic::FusionGenerator(FusionGeneratorMechanic {
        generator: id("g"),
        temperature: Some(temperature),
    })
}
fn game() -> GameState {
    GameState {
        qualities: vec!["normal".into(), "rare".into()],
        max_quality: 1,
        ..Default::default()
    }
}
fn run(dump: Value, m: &Mechanic, game: &GameState) -> Expansion<usize> {
    let store = PrototypeStore::load(&dump).unwrap();
    expand([(0, m)], &Context::new(&store, game))
}
fn fluid(name: &str, t: i32) -> DualVar {
    DualVar::Fluid {
        name: name.into(),
        temperature: [t, t],
    }
}
fn amount(e: &Expansion<usize>, key: DualVar) -> f64 {
    *e.variables[0].flow.get(&key).unwrap_or(&0.0)
}
fn close(a: f64, b: f64) {
    assert!(
        (a - b).abs() < 1e-7 * a.abs().max(b.abs()).max(1.0),
        "{a} != {b}"
    )
}
#[test]
fn fusion_reactor_uses_project_estimates_but_keeps_fuel_and_coolant_fixed() {
    let mut game = game();
    for n in [3, 5, 0] {
        game.reactor_neighbours.insert("r".into(), n);
        let e = run(dump(), &reactor(None), &game);
        assert_eq!(e.variables.len(), 1);
        close(
            amount(&e, fluid("plasma", 1000000 * (i32::from(n) + 1))),
            4.0,
        );
        close(amount(&e, fluid("cold", -150)), -4.0);
        close(amount(&e, DualVar::Item(id("cell"))), -0.0025);
        close(amount(&e, DualVar::Electricity), -10e6);
        close(e.variables[0].cost, 36.0);
    }
    let e = run(dump(), &reactor(Some(1)), &game);
    close(amount(&e, fluid("plasma", 2000000)), 4.0);
    let e = run(dump(), &reactor(None), &GameState::default());
    close(amount(&e, fluid("plasma", 4000000)), 4.0);
}
#[test]
fn fusion_quality_scales_materials_not_temperature_or_sustaining_electricity() {
    let mut m = reactor(None);
    if let Mechanic::FusionReactor(r) = &mut m {
        r.reactor.quality = "rare".into()
    }
    let mut game = game();
    game.entities_require_heating = true;
    let e = run(dump(), &m, &game);
    close(amount(&e, fluid("plasma", 4000000)), 6.4);
    close(amount(&e, fluid("cold", -150)), -6.4);
    close(amount(&e, DualVar::Item(id("cell"))), -0.004);
    close(amount(&e, DualVar::Electricity), -10e6);
    close(amount(&e, DualVar::Heat), -1e6);
}
#[test]
fn fusion_generator_limits_power_and_returns_actual_consumed_fluid() {
    for (t, rate) in [(1000000, 2.0), (4000000, 0.5), (6000000, 1.0 / 3.0)] {
        let e = run(dump(), &generator(t), &game());
        close(amount(&e, fluid("plasma", t)), -rate);
        close(amount(&e, fluid("hot", 180)), rate);
        close(amount(&e, DualVar::Electricity), 50e6);
        close(e.variables[0].cost, 15.0);
    }
    let mut m = generator(4000000);
    if let Mechanic::FusionGenerator(g) = &mut m {
        g.generator.quality = "rare".into()
    }
    let mut game = game();
    game.entities_require_heating = true;
    let e = run(dump(), &m, &game);
    close(amount(&e, DualVar::Electricity), 80e6);
    close(amount(&e, fluid("hot", 180)), 0.8);
    close(amount(&e, DualVar::Heat), -2e6);
}
#[test]
fn fusion_generator_respects_bounds_efficiency_and_burning_mode() {
    let mut d = dump();
    d["fusion-generator"]["g"]["effectivity"] = json!(0.5);
    let e = run(d.clone(), &generator(4000000), &game());
    close(amount(&e, fluid("hot", 180)), 1.0);
    d["fusion-generator"]["g"]["maximum_input_temperature"] = json!(2000000);
    assert!(
        run(d.clone(), &generator(4000000), &game())
            .variables
            .is_empty()
    );
    d["fusion-generator"]["g"]["burns_fluid"] = json!(true);
    let e = run(d, &generator(1000000), &game());
    close(amount(&e, fluid("hot", 180)), 1.0);
    close(amount(&e, DualVar::Electricity), 50e6);
    let mut d = dump();
    d["fluid"]["plasma"]["default_temperature"] = json!(100);
    let e = run(d, &generator(100), &game());
    close(amount(&e, DualVar::Electricity), 5000.0);
}
#[test]
fn invalid_explicit_fuels_cannot_generate_free_heat_or_plasma() {
    for fuel in [
        Fuel::Item { item: id("junk") },
        Fuel::Item { item: id("coal") },
        Fuel::Fluid {
            fluid: "cold".into(),
            temperature: None,
        },
    ] {
        for fusion in [true, false] {
            let r = ReactorMechanic {
                reactor: id(if fusion { "r" } else { "n" }),
                neighbours: None,
                fuel: Some(fuel.clone()),
            };
            let m = if fusion {
                Mechanic::FusionReactor(r)
            } else {
                Mechanic::Reactor(r)
            };
            assert!(run(dump(), &m, &game()).variables.is_empty());
        }
    }
}
#[test]
fn nuclear_and_fusion_share_estimate_policy_and_legacy_explicit_values() {
    let mut game = game();
    game.reactor_neighbours.insert("n".into(), 5);
    for (n, expected) in [(None, 240e6), (Some(0), 40e6)] {
        let m = Mechanic::Reactor(ReactorMechanic {
            reactor: id("n"),
            neighbours: n,
            fuel: None,
        });
        close(amount(&run(dump(), &m, &game), DualVar::Heat), expected);
    }
    let m: Mechanic = serde_json::from_value(
        json!({"type":"reactor","reactor":{"id":"n","quality":"normal"},"neighbours":0}),
    )
    .unwrap();
    assert!(matches!(
        m,
        Mechanic::Reactor(ReactorMechanic {
            neighbours: Some(0),
            ..
        })
    ));
}
#[test]
fn temperature_tables_include_neighbour_outputs_and_project_overrides() {
    let store = PrototypeStore::load(&dump()).unwrap();
    for t in [1000000, 2000000, 3000000, 4000000] {
        assert!(store.fluid_temperatures()["plasma"].contains(&t));
    }
    let table = store.fluid_temperatures_with_reactors(&[("r".into(), 7)].into());
    assert!(table["plasma"].contains(&8000000));
    assert!(!store.fluid_temperatures()["plasma"].contains(&8000000));
}

#[test]
fn vanilla_asset_fusion_flows_match_documented_rates() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data-raw-dump.json");
    let value: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let r = Mechanic::FusionReactor(FusionReactorMechanic {
        reactor: id("fusion-reactor"),
        neighbours: None,
        fuel: Some(Fuel::Item {
            item: id("fusion-power-cell"),
        }),
    });
    let e = run(value.clone(), &r, &GameState::default());
    close(amount(&e, fluid("fusion-plasma", 4000000)), 4.0);
    close(amount(&e, fluid("fluoroketone-cold", -150)), -4.0);
    close(
        amount(&e, DualVar::Item(id("fusion-power-cell"))),
        -1.0 / 400.0,
    );
    close(amount(&e, DualVar::Electricity), -10e6);
    let g = Mechanic::FusionGenerator(FusionGeneratorMechanic {
        generator: id("fusion-generator"),
        temperature: Some(4000000),
    });
    let e = run(value, &g, &GameState::default());
    close(amount(&e, fluid("fusion-plasma", 4000000)), -0.5);
    close(amount(&e, fluid("fluoroketone-hot", 180)), 0.5);
    close(amount(&e, DualVar::Electricity), 50e6);
}
