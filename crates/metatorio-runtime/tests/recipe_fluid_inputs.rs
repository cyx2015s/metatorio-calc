use metatorio_core::{Context, GameState, IdWithQuality, Mechanic, RecipeMechanic, expand::expand};
use metatorio_data::{CraftingMachineComponent, store::PrototypeStore};
use metatorio_runtime::auto_plan::{EnumerateOptions, enumerate_all};
use serde_json::{Value, json};

fn dump() -> Value {
    serde_json::from_str(include_str!(
        "../../metatorio-data/tests/fixtures/ammonia-fluid-smelting.json"
    ))
    .unwrap()
}

#[test]
fn ammonia_fluid_smelting_enumerates_only_input_capable_machines_even_when_preferred() {
    let mut raw = dump();
    raw["recipe"]["dry-smelting"] = json!({"categories":["smelting"],"ingredients":[],"results":[{"type":"item","name":"iron-plate","amount":1}]});
    let store = PrototypeStore::load(&raw).unwrap();
    let game = GameState {
        qualities: vec!["normal".into()],
        ..Default::default()
    };
    let context = Context::new(&store, &game);
    let options = EnumerateOptions {
        alternative_count: 20,
        machine_preferences: [
            "stone-furnace",
            "steel-furnace",
            "electric-furnace",
            "assembling-machine-1",
        ]
        .into_iter()
        .map(IdWithQuality::from)
        .collect(),
        enumerate_modules: vec![],
        enumerate_beacons: vec![],
        quality_limit: 0,
        major_quality: 0,
        planet: None,
        surface: None,
        accessibility: None,
    };
    let candidates = enumerate_all(&store, &context, &options);
    for recipe in ["reduce-iron-chloride", "reduce-copper-chloride"] {
        let machines: Vec<_> = candidates
            .iter()
            .filter_map(|candidate| match candidate {
                Mechanic::Recipe(m) if m.recipe.id == recipe => Some(m.machine.id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            machines,
            ["blast-furnace"],
            "{recipe}: no input-less furnace may be enumerated"
        );
    }
    assert!(candidates.iter().any(|candidate|matches!(candidate,Mechanic::Recipe(m) if m.recipe.id=="dry-smelting"&&m.machine.id=="stone-furnace")),"solid-only recipes still work in stone furnaces");
}

#[test]
fn imported_or_manual_invalid_machine_cannot_expand_fluid_recipe() {
    let store = PrototypeStore::load(&dump()).unwrap();
    let game = GameState {
        qualities: vec!["normal".into()],
        ..Default::default()
    };
    let context = Context::new(&store, &game);
    for (machine, valid) in [
        ("stone-furnace", false),
        ("steel-furnace", false),
        ("electric-furnace", false),
        ("assembling-machine-1", false),
        ("blast-furnace", true),
    ] {
        let mechanic = Mechanic::Recipe(RecipeMechanic {
            recipe: "reduce-iron-chloride".into(),
            machine: machine.into(),
            ..Default::default()
        });
        assert_eq!(
            !expand([(0, &mechanic)], &context).variables.is_empty(),
            valid,
            "{machine}"
        );
    }
    assert!(
        store
            .entity("assembling-machine-2")
            .unwrap()
            .component::<CraftingMachineComponent>()
            .unwrap()
            .supports_fluid_input()
    );
}
