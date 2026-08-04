//! Mechanic 枚举的集成测试（外部 crate 视角：验证 non_exhaustive 与 serde 行为）。

use metatorio_core::{
    BeaconConfig, BoilerMechanic, GeneratorMechanic, IdWithQuality, Mechanic, MiningMechanic,
    ModuleConfig, NORMAL_QUALITY, RecipeMechanic,
};

fn id(s: &str) -> IdWithQuality {
    IdWithQuality(s.to_string(), NORMAL_QUALITY.to_string())
}

#[test]
fn recipe_mechanic_serde_roundtrip() {
    // Mechanic 是工厂的单个组件：1 配方 + 1 机器 + 可选燃料 + 插件配置
    let m = Mechanic::Recipe(RecipeMechanic {
        recipe: id("iron-plate"),
        machine: id("assembling-machine-2"),
        module_config: ModuleConfig {
            modules: vec![id("productivity-module-2")],
            ..Default::default()
        },
        fuel: None,
    });
    let json = serde_json::to_string(&m).unwrap();
    // tag = "type"，变体名 kebab-case："recipe"
    assert!(json.contains(r#""type":"recipe""#), "json: {json}");
    assert!(json.contains(r#""recipe":["iron-plate","normal"]"#), "json: {json}");
    let back: Mechanic = serde_json::from_str(&json).unwrap();
    assert_eq!(m, back);
}

#[test]
fn generator_mechanic_serde_roundtrip() {
    let m = Mechanic::Generator(GeneratorMechanic {
        generator: id("steam-engine"),
        fluid: "steam".to_string(),
        temperature: 165,
    });
    let json = serde_json::to_string(&m).unwrap();
    assert!(json.contains(r#""type":"generator""#), "json: {json}");
    let back: Mechanic = serde_json::from_str(&json).unwrap();
    assert_eq!(m, back);
}

#[test]
fn missing_fields_default() {
    // struct 级 #[serde(default)]：缺字段容错（mod 数据污染场景）
    let m: Mechanic = serde_json::from_str(r#"{"type": "mining"}"#).unwrap();
    match m {
        Mechanic::Mining(mining) => {
            assert_eq!(mining.resource, "");
            assert_eq!(mining.machine, IdWithQuality::default());
            assert!(mining.fuel.is_none());
        }
        _ => panic!("expected mining"),
    }
    // 组件级缺字段容错
    let b: BoilerMechanic = serde_json::from_str(r#"{"boiler": ["boiler", "normal"]}"#).unwrap();
    assert_eq!(b.temperature, 0);
    assert!(b.fuel.is_none());
}

#[test]
fn non_exhaustive_requires_wildcard() {
    // 外部 crate 对 #[non_exhaustive] 枚举必须包含 `_` 分支——能编译即验证
    let m: Mechanic = serde_json::from_str(r#"{"type": "reactor"}"#).unwrap();
    let _ = match m {
        Mechanic::Recipe(_) => 1,
        Mechanic::Mining(_) => 2,
        Mechanic::Spoil(_) => 3,
        Mechanic::Plant(_) => 4,
        Mechanic::ItemFuel(_) => 5,
        Mechanic::ItemLaunch(_) => 6,
        Mechanic::Generator(_) => 7,
        Mechanic::Boiler(_) => 8,
        Mechanic::FluidFuel(_) => 9,
        Mechanic::FluidHeat(_) => 10,
        Mechanic::Reactor(_) => 11,
        _ => 99, // non_exhaustive：未来新增变体
    };
}

#[test]
fn id_with_quality_uses_string() {
    let a = IdWithQuality("iron-ore".to_string(), "uncommon".to_string());
    let json = serde_json::to_string(&a).unwrap();
    assert_eq!(json, r#"["iron-ore","uncommon"]"#);
    let back: IdWithQuality = serde_json::from_str(&json).unwrap();
    assert_eq!(back, a);
    // From<&str> 默认 normal 品质
    let n: IdWithQuality = "copper-ore".into();
    assert_eq!(n, IdWithQuality("copper-ore".to_string(), "normal".to_string()));
}

#[test]
fn beacon_config_fields() {
    // BeaconConfig 迁移完整性：module/beacon/count/share 都在
    let b = BeaconConfig {
        modules: vec![(id("speed-module-3"), 2)],
        beacon: id("beacon"),
        count: 4,
        share: 0.5,
    };
    let json = serde_json::to_string(&b).unwrap();
    let back: BeaconConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(b, back);
}
