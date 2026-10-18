use metatorio_core::expand::expand;
use metatorio_core::{
    Context, DualVar, GameState, IdWithQuality, Mechanic, SolarMechanic, solar_balance,
};
use metatorio_data::store::PrototypeStore;
use metatorio_runtime::{
    auto_plan::{EnumerateOptions, enumerate_all},
    solve::apply_environment_to_game_state,
};
use serde_json::json;

fn fixture() -> PrototypeStore {
    PrototypeStore::load(&json!({
        "planet": {
            "home":{"solar_power_in_space":300,"surface_properties":{"solar-power":100,"day-night-cycle":25200}},
            "short-day":{"solar_power_in_space":120,"surface_properties":{"solar-power":20,"day-night-cycle":10800}},
            "defaults":{}
        },
        "space-location":{"deep":{"solar_power_in_space":50}},
        "surface":{"custom-orbit":{"surface_properties":{"solar-power":999,"day-night-cycle":12345,"glow":30}}},
        "surface-property":{"solar-power":{"default_value":50},"day-night-cycle":{"default_value":3600},"glow":{"default_value":10}},
        "solar-panel":{
            "panel":{"production":"60kW","collision_box":[[-1.5,-1.5],[1.5,1.5]]},
            "custom":{"production":"60kW","solar_coefficient_property":"glow"}
        },
        "accumulator":{"storage":{"energy_source":{"type":"electric","buffer_capacity":"5MJ"},"collision_box":[[-1,-1],[1,1]]}},
        "quality":{"normal":{"level":0},"rare":{"level":2,"accumulator_capacity_multiplier":3}}
    })).unwrap()
}

fn game(store: &PrototypeStore, location: &str, surface: Option<&str>) -> GameState {
    let mut game = GameState::default();
    apply_environment_to_game_state(store, &mut game, Some(location), surface);
    game
}

fn panel(storage: &str) -> SolarMechanic {
    SolarMechanic {
        solar_panel: IdWithQuality::from("panel"),
        accumulator: IdWithQuality::from(storage),
    }
}

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < b.abs().max(1.0) * 1e-10, "{a} != {b}");
}

#[test]
fn orbit_uses_location_percentage_and_does_not_require_storage() {
    let store = fixture();
    for (location, power) in [
        ("home", 180_000.0),
        ("short-day", 72_000.0),
        ("deep", 30_000.0),
    ] {
        let state = game(&store, location, Some("custom-orbit"));
        let ctx = Context::new(&store, &state);
        for storage in ["", "missing", "storage"] {
            let mechanic = panel(storage);
            let balance = solar_balance(&ctx, &mechanic).unwrap();
            assert!(balance.continuous_daylight);
            near(balance.average_power, power);
            near(balance.cycle_seconds, 0.0);
            near(balance.surplus_per_cycle, 0.0);
            near(balance.recommended_accumulators, 0.0);
            let expansion = expand([(0u32, &Mechanic::Solar(mechanic))], &ctx);
            assert_eq!(expansion.variables.len(), 1);
            near(expansion.variables[0].flow[&DualVar::Electricity], power);
            near(expansion.variables[0].cost, 9.0);
        }
    }
}

#[test]
fn ground_keeps_ticks_and_uses_storage_quality_and_property_defaults() {
    let store = fixture();
    for (location, power, cycle) in [
        ("home", 42_000.0, 420.0),
        ("short-day", 8_400.0, 180.0),
        ("defaults", 21_000.0, 60.0),
    ] {
        let state = game(&store, location, None);
        let ctx = Context::new(&store, &state);
        let mechanic = panel("storage");
        let balance = solar_balance(&ctx, &mechanic).unwrap();
        assert!(!balance.continuous_daylight);
        near(balance.average_power, power);
        near(balance.cycle_seconds, cycle);
        assert!(balance.recommended_accumulators > 0.0);
        let mut rare = mechanic.clone();
        rare.accumulator.quality = "rare".into();
        let rare_balance = solar_balance(&ctx, &rare).unwrap();
        near(rare_balance.accumulator_capacity, 15_000_000.0);
        near(
            rare_balance.recommended_accumulators,
            balance.recommended_accumulators / 3.0,
        );
        let expansion = expand([(0u32, &Mechanic::Solar(rare))], &ctx);
        near(expansion.variables[0].flow[&DualVar::Electricity], power);
        near(
            expansion.variables[0].cost,
            9.0 + 4.0 * rare_balance.recommended_accumulators,
        );
        assert!(solar_balance(&ctx, &panel("")).is_none());
        assert!(
            expand([(0u32, &Mechanic::Solar(panel("missing")))], &ctx)
                .variables
                .is_empty()
        );
    }
}

#[test]
fn custom_property_overrides_orbit_coefficient_and_environment_switch_resets_state() {
    let store = fixture();
    let mut state = game(&store, "home", Some("custom-orbit"));
    let mut mechanic = panel("");
    mechanic.solar_panel.id = "custom".into();
    near(
        solar_balance(&Context::new(&store, &state), &mechanic)
            .unwrap()
            .average_power,
        180_000.0,
    );
    apply_environment_to_game_state(&store, &mut state, Some("short-day"), None);
    assert!(!state.solar_continuous_daylight);
    assert!(!state.surface_properties.contains_key("glow"));
    mechanic.accumulator.id = "storage".into();
    near(
        solar_balance(&Context::new(&store, &state), &mechanic)
            .unwrap()
            .average_power,
        42_000.0,
    );
}

#[test]
fn orbit_enumeration_works_without_any_accumulator_prototype() {
    let store = PrototypeStore::load(&json!({
        "planet":{"home":{"solar_power_in_space":300}},
        "solar-panel":{"panel":{"production":"60kW"}},
        "quality":{"normal":{"level":0}}
    }))
    .unwrap();
    let state = game(&store, "home", Some("space-platform"));
    let ctx = Context::new(&store, &state);
    let options = EnumerateOptions {
        planet: Some("home".into()),
        surface: Some("space-platform".into()),
        alternative_count: 1,
        machine_preferences: Vec::new(),
        enumerate_modules: Vec::new(),
        enumerate_beacons: Vec::new(),
        quality_limit: 0,
        major_quality: 0,
        accessibility: None,
    };
    let candidates = enumerate_all(&store, &ctx, &options);
    let solar: Vec<_> = candidates
        .iter()
        .filter_map(|m| {
            if let Mechanic::Solar(s) = m {
                Some(s)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(solar.len(), 1);
    assert!(solar[0].accumulator.id.is_empty());
    near(
        solar_balance(&ctx, solar[0]).unwrap().average_power,
        180_000.0,
    );
}

#[test]
fn real_assets_match_ground_and_orbit_power() {
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
    let mechanic = SolarMechanic {
        solar_panel: IdWithQuality::from("solar-panel"),
        accumulator: IdWithQuality::from("accumulator"),
    };
    for (location, ground, orbit, cycle) in [
        ("nauvis", 42_000.0, 180_000.0, 420.0),
        ("fulgora", 8_400.0, 72_000.0, 180.0),
        ("vulcanus", 168_000.0, 360_000.0, 90.0),
    ] {
        let state = game(&store, location, None);
        let balance = solar_balance(&Context::new(&store, &state), &mechanic).unwrap();
        near(balance.average_power, ground);
        near(balance.cycle_seconds, cycle);
        let state = game(&store, location, Some("space-platform"));
        let balance = solar_balance(&Context::new(&store, &state), &mechanic).unwrap();
        near(balance.average_power, orbit);
        near(balance.recommended_accumulators, 0.0);
    }
}
