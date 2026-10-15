use metatorio_core::{
    Context, DualVar, GameState, IdWithQuality, Mechanic, VirtualScienceMechanic,
};
use metatorio_core::{
    expand::expand,
    planet::{implicit_source_flows, orbit_asteroid_flows},
};
use metatorio_data::store::PrototypeStore;
use serde_json::json;

#[test]
fn orbit_sources_follow_fragment_chains_and_do_not_leak_between_locations() {
    let store = PrototypeStore::load(&json!({
        "item": {"iron-chunk": {}, "ice-chunk": {}, "other-chunk": {}, "disabled-chunk": {}},
        "fluid": {"water": {}},
        "planet": {
            "home": {"asteroid_spawn_definitions": [
                {"asteroid": "direct", "probability": 1},
                {"asteroid": "big", "probability": 1},
                {"asteroid": "disabled", "probability": 0}
            ]},
            "other": {"asteroid_spawn_definitions": [{"asteroid": "foreign", "probability": 1}]}
        },
        "space-connection": {"route": {"from": "home", "to": "other", "asteroid_spawn_definitions": [{"asteroid": "foreign"}]}},
        "asteroid": {
            "big": {"damaged_trigger_effect": {"type": "create-entity", "entity_name": "small"}},
            "small": {"dying_trigger_effect": [
                {"type": "create-asteroid-chunk", "asteroid_name": "ice"},
                {"type": "create-entity", "entity_name": "big"}
            ]}
        },
        "asteroid-chunk": {
            "direct": {"minable": {"mining_time": 1, "result": "iron-chunk"}},
            "ice": {"minable": {"mining_time": 1, "results": [
                {"type": "item", "name": "ice-chunk", "amount": 2},
                {"type": "fluid", "name": "water", "amount": 100}
            ]}},
            "foreign": {"minable": {"mining_time": 1, "result": "other-chunk"}},
            "disabled": {"minable": {"mining_time": 1, "result": "disabled-chunk"}}
        }
    })).unwrap();
    let expected: metatorio_core::Flow = ["iron-chunk", "ice-chunk"]
        .into_iter()
        .map(|name| (DualVar::Item(IdWithQuality::new(name, "normal")), 0.0))
        .collect();
    assert_eq!(orbit_asteroid_flows(&store, "home"), expected);
    assert_eq!(
        implicit_source_flows(&store, "home", Some("space-platform")),
        expected
    );
    assert!(implicit_source_flows(&store, "home", None).is_empty());
    assert!(implicit_source_flows(&store, "home", Some("custom-surface")).is_empty());
    let other = orbit_asteroid_flows(&store, "other");
    assert_eq!(other.len(), 1);
    assert!(other.contains_key(&DualVar::Item(IdWithQuality::new("other-chunk", "normal"))));
    assert!(orbit_asteroid_flows(&store, "missing").is_empty());
}

#[test]
fn vanilla_platform_gets_normal_chunk_items_instead_of_ground_resource_entities() {
    let dump = serde_json::from_str(include_str!("../../../assets/data-raw-dump.json")).unwrap();
    let store = PrototypeStore::load(&dump).unwrap();
    let orbit = implicit_source_flows(&store, "nauvis", Some("space-platform"));
    assert_eq!(orbit.len(), 3);
    for name in [
        "metallic-asteroid-chunk",
        "carbonic-asteroid-chunk",
        "oxide-asteroid-chunk",
    ] {
        assert_eq!(
            orbit[&DualVar::Item(IdWithQuality::new(name, "normal"))],
            0.0
        );
    }
    assert!(
        implicit_source_flows(&store, "nauvis", None)
            .keys()
            .any(|flow| matches!(flow, DualVar::Entity(_)))
    );
    assert!(!orbit.keys().any(|flow| matches!(flow, DualVar::Entity(_))));
}

#[test]
fn science_values_use_capacity_quality_and_tool_durability_without_machine_bonuses() {
    let store = PrototypeStore::load(&json!({
        "item": {"red": {"science_capacity": 2}, "tech-only": {}, "not-science": {}},
        "tool": {"tool-pack": {"durability": 10}},
        "lab": {"lab": {"inputs": ["red", "tool-pack"]}},
        "technology": {"research": {"unit": {"ingredients": [["tech-only", 1]]}}},
        "quality": {
            "normal": {"level": 0}, "uncommon": {"level": 1},
            "rare": {"level": 2}, "epic": {"level": 3}, "legendary": {"level": 5},
            "custom": {"level": 10, "tool_durability_multiplier": 3, "science_capacity_multiplier": 7,
                "default_multiplier": 999, "science_pack_drain_multiplier": 0.01}
        }
    })).unwrap();
    let game = GameState::default();
    let ctx = Context::new(&store, &game);
    for (quality, multiplier) in [
        ("normal", 1.0),
        ("uncommon", 2.0),
        ("rare", 3.0),
        ("epic", 4.0),
        ("legendary", 6.0),
        ("custom", 7.0),
    ] {
        let id = IdWithQuality::new("red", quality);
        let mechanic = Mechanic::VirtualScience(VirtualScienceMechanic { item: id.clone() });
        let expansion = expand([(0, &mechanic)], &ctx);
        assert_eq!(expansion.len(), 1);
        let variable = &expansion.variables[0];
        assert_eq!(variable.cost, 0.0);
        assert_eq!(variable.flow.len(), 2);
        assert_eq!(variable.flow[&DualVar::Item(id)], -1.0);
        assert_eq!(
            variable.flow[&DualVar::VirtualScience("red".into())],
            2.0 * multiplier
        );
        let serialized = serde_json::to_value(&mechanic).unwrap();
        assert_eq!(
            serde_json::from_value::<Mechanic>(serialized).unwrap(),
            mechanic
        );
    }
    assert_eq!(
        metatorio_core::science::science_value(&store, &IdWithQuality::new("tool-pack", "custom")),
        Some(30.0)
    );
    assert!(metatorio_core::science::science_pack_names(&store).contains("tech-only"));
    let invalid = Mechanic::VirtualScience(VirtualScienceMechanic {
        item: IdWithQuality::new("not-science", "normal"),
    });
    assert!(expand([(0, &invalid)], &ctx).is_empty());
    assert_eq!(
        serde_json::to_value(DualVar::VirtualScience("red".into())).unwrap(),
        json!({"VirtualScience": "red"})
    );
}
