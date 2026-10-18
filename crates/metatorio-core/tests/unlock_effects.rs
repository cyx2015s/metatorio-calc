use metatorio_core::{AccessibilityOptions, Accessible, compute_accessibility, milestone_order};
use metatorio_data::store::PrototypeStore;
use serde_json::json;

fn fixture() -> PrototypeStore {
    PrototypeStore::load(&json!({
        "planet": {"nauvis": {}, "same": {}, "orphan": {}, "either": {}},
        "space-location": {"deep": {}},
        "quality": {"normal": {"level":0}, "same": {"level":1}},
        "recipe": {"same": {"enabled":false}, "deep": {"enabled":false}},
        "technology": {
            "arbitrary-expedition": {"effects":[{"type":"unlock-space-location","space_location":"same"}]},
            "planet-discovery-orphan": {},
            "recipe-unlock": {"effects":[{"type":"unlock-recipe","recipe":"same"}]},
            "quality-unlock": {"effects":[{"type":"unlock-quality","quality":"same"}]},
            "expedition-a": {"effects":[{"type":"unlock-space-location","space_location":"either"}]},
            "expedition-b": {"effects":[{"type":"unlock-space-location","space_location":"either"}]},
            "deep-expedition": {"effects":[{"type":"unlock-space-location","space_location":"deep"}]}
        }
    })).unwrap()
}

#[test]
fn location_unlocks_follow_effects_and_milestones_not_technology_names() {
    let store = fixture();
    let result = compute_accessibility(&store, &AccessibilityOptions::default());
    assert!(result.is_accessible(&Accessible::Planet("same".into())));
    assert!(result.is_accessible(&Accessible::Space("deep".into())));
    assert!(!result.is_accessible(&Accessible::Planet("orphan".into())));
    let options = AccessibilityOptions {
        forced_inaccessible: [Accessible::Tech("arbitrary-expedition".into())]
            .into_iter()
            .collect(),
        ..Default::default()
    };
    let result = compute_accessibility(&store, &options);
    assert!(!result.is_accessible(&Accessible::Planet("same".into())));
    assert!(result.is_accessible(&Accessible::Recipe("same".into())));
    assert!(result.is_accessible(&Accessible::Quality("same".into())));
    // A space-location unlock cannot unlock an unrelated recipe with that name.
    assert!(!result.is_accessible(&Accessible::Recipe("deep".into())));
    let nodes = vec![
        Accessible::Planet("same".into()),
        Accessible::Tech("arbitrary-expedition".into()),
    ];
    assert_eq!(
        milestone_order(&store, &nodes),
        vec![nodes[1].clone(), nodes[0].clone()]
    );
}

#[test]
fn alternative_unlocks_are_or_dependencies_and_kinds_are_isolated() {
    let store = fixture();
    let mut options = AccessibilityOptions {
        forced_inaccessible: ["expedition-a", "recipe-unlock", "quality-unlock"]
            .map(|name| Accessible::Tech(name.into()))
            .into_iter()
            .collect(),
        ..Default::default()
    };
    let result = compute_accessibility(&store, &options);
    assert!(result.is_accessible(&Accessible::Planet("either".into())));
    assert!(result.is_accessible(&Accessible::Planet("same".into())));
    assert!(!result.is_accessible(&Accessible::Recipe("same".into())));
    assert!(!result.is_accessible(&Accessible::Quality("same".into())));
    options
        .forced_inaccessible
        .insert(Accessible::Tech("expedition-b".into()));
    assert!(
        !compute_accessibility(&store, &options)
            .is_accessible(&Accessible::Planet("either".into()))
    );
}

#[test]
fn explicit_nauvis_unlock_takes_precedence_over_default_start() {
    let store = PrototypeStore::load(&json!({
        "planet":{"nauvis":{}},
        "technology":{"modded-start":{"effects":[{"type":"unlock-space-location","space_location":"nauvis"}]}}
    })).unwrap();
    let options = AccessibilityOptions {
        forced_inaccessible: [Accessible::Tech("modded-start".into())]
            .into_iter()
            .collect(),
        ..Default::default()
    };
    assert!(
        !compute_accessibility(&store, &options)
            .is_accessible(&Accessible::Planet("nauvis".into()))
    );
}

#[test]
fn vanilla_planet_tech_can_be_renamed_without_changing_accessibility() {
    let mut dump: serde_json::Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/data-raw-dump.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let before = PrototypeStore::load(&dump).unwrap();
    let original = compute_accessibility(&before, &AccessibilityOptions::default());
    let renamed = "arbitrary-mod-expedition";
    let tech = dump["technology"]
        .as_object_mut()
        .unwrap()
        .remove("planet-discovery-vulcanus")
        .unwrap();
    dump["technology"][renamed] = tech;
    dump["technology"][renamed]["name"] = json!(renamed);
    for tech in dump["technology"].as_object_mut().unwrap().values_mut() {
        if let Some(prerequisites) = tech
            .get_mut("prerequisites")
            .and_then(serde_json::Value::as_array_mut)
        {
            for prerequisite in prerequisites {
                if prerequisite == "planet-discovery-vulcanus" {
                    *prerequisite = json!(renamed);
                }
            }
        }
    }
    let store = PrototypeStore::load(&dump).unwrap();
    let result = compute_accessibility(&store, &AccessibilityOptions::default());
    let planet = Accessible::Planet("vulcanus".into());
    assert!(original.is_accessible(&planet));
    assert!(result.is_accessible(&planet));
    let locked = AccessibilityOptions {
        forced_inaccessible: [Accessible::Tech(renamed.into())].into_iter().collect(),
        ..Default::default()
    };
    assert!(!compute_accessibility(&store, &locked).is_accessible(&planet));
}
