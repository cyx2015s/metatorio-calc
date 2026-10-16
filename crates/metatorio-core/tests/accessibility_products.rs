use metatorio_core::{
    AccessibilityOptions, Accessible, build_graph, compute_accessibility,
    compute_accessibility_with_graph, milestone_order_with_graph,
};
use metatorio_data::ItemComponent;
use metatorio_data::store::{PrototypeGroup, PrototypeStore};
use serde_json::{Value, json};

fn fixture() -> Value {
    json!({
        "item": {
            "fuel": {"fuel_value": "1MJ", "fuel_categories": ["chemical"], "burnt_result": "ash"},
            "ash": {"fuel_value": "1MJ", "fuel_categories": ["chemical"], "burnt_result": "slag"},
            "slag": {}, "pack": {}, "water": {},
            "locked-fuel": {"fuel_value": "1MJ", "fuel_categories": ["chemical"], "burnt_result": "locked-ash"},
            "locked-ash": {},
            "invalid-fuel": {"burnt_result": "ghost"}, "ghost": {},
            "cycle-a": {"fuel_value": "1MJ", "fuel_categories": ["chemical"], "burnt_result": "cycle-b"},
            "cycle-b": {"fuel_value": "1MJ", "fuel_categories": ["chemical"], "burnt_result": "cycle-a"}
        },
        "fluid": {"water": {}, "acid": {}},
        "tile": {"lake": {"fluid": "water"}, "acid-lake": {"fluid": "acid"}},
        "planet": {
            "nauvis": {"map_gen_settings": {"autoplace_settings": {"tile": {"settings": {"lake": {}}}}}},
            "remote": {"map_gen_settings": {"autoplace_settings": {"tile": {"settings": {"acid-lake": {}}}}}}
        },
        "recipe": {
            "fuel": {"enabled": true, "results": [{"type": "item", "name": "fuel", "amount": 1}]},
            "pack": {"enabled": true, "ingredients": [{"type": "item", "name": "ash", "amount": 1}, {"type": "fluid", "name": "water", "amount": 1}], "results": [{"type": "item", "name": "pack", "amount": 1}]}
        },
        "technology": {"advanced": {"unit": {"count": 1, "time": 1, "ingredients": [["pack", 1]]}}}
    })
}

#[test]
fn burnt_and_tile_products_propagate_into_the_recorded_set_and_technology_dependencies() {
    let store = PrototypeStore::load(&fixture()).unwrap();
    let graph = build_graph(&store);
    let result = compute_accessibility_with_graph(&store, &AccessibilityOptions::default(), &graph);
    for node in [
        Accessible::Item("fuel".into()),
        Accessible::Item("ash".into()),
        Accessible::Item("slag".into()),
        Accessible::Fluid("water".into()),
        Accessible::Item("pack".into()),
        Accessible::Tech("advanced".into()),
    ] {
        assert!(result.accessible().contains(&node), "{node:?}");
    }
    for node in [
        Accessible::Item("water".into()),
        Accessible::Item("locked-ash".into()),
        Accessible::Item("ghost".into()),
        Accessible::Item("cycle-a".into()),
        Accessible::Fluid("acid".into()),
    ] {
        assert!(!result.is_accessible(&node), "{node:?}");
    }
    let milestones = [
        Accessible::Tech("advanced".into()),
        Accessible::Item("ash".into()),
        Accessible::Fluid("water".into()),
        Accessible::Item("fuel".into()),
    ];
    let ordered = milestone_order_with_graph(&store, &graph, &milestones);
    let position = |node: &Accessible| {
        ordered
            .iter()
            .position(|candidate| candidate == node)
            .unwrap()
    };
    assert!(position(&milestones[3]) < position(&milestones[1]));
    assert!(position(&milestones[1]) < position(&milestones[0]));
    assert!(position(&milestones[2]) < position(&milestones[0]));
}

#[test]
fn forced_overrides_and_alternative_burnt_sources_work_with_the_cached_graph() {
    let mut dump = fixture();
    dump["item"]["backup-fuel"] = dump["item"]["fuel"].clone();
    let store = PrototypeStore::load(&dump).unwrap();
    let graph = build_graph(&store);
    let mut options = AccessibilityOptions::default();
    options
        .forced_inaccessible
        .insert(Accessible::Item("fuel".into()));
    options
        .forced_accessible
        .insert(Accessible::Item("invalid-fuel".into()));
    let blocked = compute_accessibility_with_graph(&store, &options, &graph);
    for item in ["fuel", "ash", "slag", "pack"] {
        assert!(!blocked.is_item_accessible(item));
    }
    assert!(!blocked.is_accessible(&Accessible::Tech("advanced".into())));
    assert!(!blocked.is_item_accessible("ghost"));
    options
        .forced_accessible
        .insert(Accessible::Item("backup-fuel".into()));
    options
        .forced_accessible
        .insert(Accessible::Planet("remote".into()));
    let forced = compute_accessibility_with_graph(&store, &options, &graph);
    assert!(forced.is_item_accessible("slag"));
    assert!(forced.is_accessible(&Accessible::Tech("advanced".into())));
    assert!(forced.is_accessible(&Accessible::Fluid("acid".into())));
    options
        .forced_inaccessible
        .insert(Accessible::Fluid("water".into()));
    let no_water = compute_accessibility_with_graph(&store, &options, &graph);
    assert!(!no_water.is_item_accessible("pack"));
    assert!(!no_water.is_accessible(&Accessible::Tech("advanced".into())));
}

#[test]
fn referenced_products_are_nodes_even_when_their_prototype_type_is_not_loaded() {
    let mut dump = fixture();
    dump["item"]["fuel"]["burnt_result"] = json!("untyped-ash");
    dump["tile"]["lake"]["fluid"] = json!("untyped-water");
    let store = PrototypeStore::load(&dump).unwrap();
    for all_accessible in [false, true] {
        let result = compute_accessibility(
            &store,
            &AccessibilityOptions {
                all_accessible,
                ..Default::default()
            },
        );
        assert!(result.is_item_accessible("untyped-ash"));
        assert!(result.is_accessible(&Accessible::Fluid("untyped-water".into())));
    }
}

#[test]
fn tile_outputs_follow_recursive_terrain_reachability_on_ground_and_platforms() {
    let mut dump = fixture();
    dump["tile"]["deck"] = json!({});
    dump["item"]["deck"] = json!({"place_as_tile": {"result": "deck", "condition": {"layers": {}}, "tile_condition": ["lake"]}});
    dump["item"]["acid-lake"] = json!({"place_as_tile": {"result": "acid-lake", "condition": {"layers": {}}, "tile_condition": ["deck"]}});
    let store = PrototypeStore::load(&dump).unwrap();
    assert!(
        compute_accessibility(&store, &AccessibilityOptions::default())
            .is_accessible(&Accessible::Fluid("acid".into()))
    );
    dump["tile"]["deck"]["allows_being_covered"] = json!(false);
    let blocked = PrototypeStore::load(&dump).unwrap();
    assert!(
        !compute_accessibility(&blocked, &AccessibilityOptions::default())
            .is_accessible(&Accessible::Fluid("acid".into()))
    );
    // 独立轨道地点：平台初始地格也能成为抽取来源，仍需地点解锁。
    dump["planet"] = json!({});
    dump["space-location"] = json!({"orbit": {}});
    dump["space-platform-starter-pack"] = json!({"starter": {"tiles": [{"tile": "acid-lake"}]}});
    let space = PrototypeStore::load(&dump).unwrap();
    assert!(
        !compute_accessibility(&space, &AccessibilityOptions::default())
            .is_accessible(&Accessible::Fluid("acid".into()))
    );
    let mut options = AccessibilityOptions::default();
    options
        .forced_accessible
        .insert(Accessible::Space("orbit".into()));
    assert!(
        compute_accessibility(&space, &options).is_accessible(&Accessible::Fluid("acid".into()))
    );
}

#[test]
fn real_assets_expose_burnt_fuel_products_and_nauvis_water() {
    for raw in [
        include_str!("../../../assets/data-raw-dump.json"),
        include_str!("../../../assets/data-raw-dump-heavily-modded.json"),
    ] {
        let store = PrototypeStore::load(&serde_json::from_str(raw).unwrap()).unwrap();
        let graph = build_graph(&store);
        let mut options = AccessibilityOptions::default();
        let mut products = Vec::new();
        for record in store.group(PrototypeGroup::Item) {
            let item = record.component::<ItemComponent>().unwrap();
            if item.fuel_value().amount > 0.0
                && !item.fuel_categories.is_empty()
                && !item.burnt_result.is_empty()
            {
                options
                    .forced_accessible
                    .insert(Accessible::Item(record.name.clone()));
                products.push(item.burnt_result.clone());
            }
        }
        assert!(!products.is_empty());
        let result = compute_accessibility_with_graph(&store, &options, &graph);
        for product in products {
            assert!(result.is_item_accessible(&product), "{product}");
        }
        assert!(result.is_accessible(&Accessible::Fluid("water".into())));
    }
}
