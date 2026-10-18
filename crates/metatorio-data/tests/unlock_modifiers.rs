use metatorio_data::{
    TechnologyComponent,
    store::{PrototypeGroup, PrototypeStore},
    types::Modifier,
};
use serde_json::json;

#[test]
fn space_platform_modifier_tag_is_checked_against_the_pinned_schema() {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../schema/prototype-api.json")).unwrap();
    let types = schema["types"].as_array().unwrap();
    let platform = types
        .iter()
        .find(|t| t["name"] == "SpacePlatformsModifier")
        .unwrap();
    let tag = platform["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "type")
        .unwrap()["type"]["value"]
        .as_str()
        .unwrap();
    let effect: Modifier = serde_json::from_value(json!({"type":tag,"modifier":true})).unwrap();
    assert!(matches!(effect, Modifier::SpacePlatforms(_)));
    // The required flag belongs to the parent, not the concrete modifier type.
    assert_eq!(platform["parent"], "BoolModifier");
    let parent = types.iter().find(|t| t["name"] == "BoolModifier").unwrap();
    assert!(
        parent["properties"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "modifier" && p["optional"] == false)
    );
    assert!(serde_json::from_value::<Modifier>(json!({"type":tag})).is_err());
}

#[test]
fn unlock_effect_tags_and_flag_fields_match_the_prototype_schema() {
    for modifier in [false, true] {
        let effects = json!([
            {"type":"mining-with-fluid","modifier":modifier},
            {"type":"unlock-space-platforms","modifier":modifier},
            {"type":"unlock-space-location","space_location":"custom-planet"}
        ]);
        let store =
            PrototypeStore::load(&json!({"technology":{"arbitrary":{"effects":effects}}})).unwrap();
        let technology = store
            .get(PrototypeGroup::Technology, "arbitrary")
            .unwrap()
            .component::<TechnologyComponent>()
            .unwrap();
        assert!(
            matches!(&technology.effects[0], Modifier::MiningWithFluid(flag) if flag.modifier == modifier)
        );
        assert!(
            matches!(&technology.effects[1], Modifier::SpacePlatforms(flag) if flag.modifier == modifier)
        );
        assert!(
            matches!(&technology.effects[2], Modifier::UnlockSpaceLocation(effect) if effect.space_location == "custom-planet")
        );
        assert_eq!(
            serde_json::to_value(&technology.effects[1]).unwrap()["type"],
            "unlock-space-platforms"
        );
    }
}
