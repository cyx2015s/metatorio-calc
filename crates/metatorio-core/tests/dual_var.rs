//! DualVar 流标识的测试：serde 往返、FluidHeat 纯筛选、ItemFuel 燃尽产物标记。

use metatorio_core::{DualVar, IdWithQuality};

#[test]
fn fluid_serde_roundtrip() {
    // 流体流承载温度
    let f = DualVar::Fluid {
        name: "steam".to_string(),
        temperature: [165; 2],
    };
    let json = serde_json::to_string(&f).unwrap();
    assert!(json.contains(r#""Fluid""#), "json: {json}");
    assert!(json.contains("temperature"), "json: {json}");
    let back: DualVar = serde_json::from_str(&json).unwrap();
    assert_eq!(f, back);
}

#[test]
fn fluid_heat_pure_filter() {
    // 纯筛选：绑定流体
    let h = DualVar::FluidHeat {
        filter: "steam".to_string(),
    };
    let json = serde_json::to_string(&h).unwrap();
    assert!(json.contains(r#""FluidHeat""#), "json: {json}");
    assert!(json.contains(r#""filter":"steam""#), "json: {json}");
    let back: DualVar = serde_json::from_str(&json).unwrap();
    assert_eq!(h, back);
}

#[test]
fn item_fuel_has_burnt_result() {
    let f = DualVar::ItemFuel {
        category: vec!["chemical".to_string()],
        has_burnt_result: true,
    };
    let json = serde_json::to_string(&f).unwrap();
    assert!(json.contains(r#""has_burnt_result":true"#), "json: {json}");
    let back: DualVar = serde_json::from_str(&json).unwrap();
    assert_eq!(f, back);

    // 缺 has_burnt_result → false（serde default）
    let f: DualVar = serde_json::from_str(r#"{"ItemFuel": {"category": ["chemical"]}}"#).unwrap();
    assert_eq!(
        f,
        DualVar::ItemFuel {
            category: vec!["chemical".to_string()],
            has_burnt_result: false
        }
    );
}

#[test]
fn is_energy() {
    assert!(DualVar::Heat.is_energy());
    assert!(DualVar::Electricity.is_energy());
    assert!(
        DualVar::FluidHeat {
            filter: "steam".to_string()
        }
        .is_energy()
    );
    assert!(
        DualVar::ItemFuel {
            category: vec!["chemical".to_string()],
            has_burnt_result: false
        }
        .is_energy()
    );
    assert!(!DualVar::Item(IdWithQuality::default()).is_energy());
    assert!(
        !DualVar::Fluid {
            name: "water".to_string(),
            temperature: [15; 2],
        }
        .is_energy()
    );
    assert!(!DualVar::RocketSlotCapacity.is_energy());
}

#[test]
fn item_serde_roundtrip() {
    let i = DualVar::Item(IdWithQuality::new("iron-plate", "normal"));
    let json = serde_json::to_string(&i).unwrap();
    let back: DualVar = serde_json::from_str(&json).unwrap();
    assert_eq!(i, back);
}
