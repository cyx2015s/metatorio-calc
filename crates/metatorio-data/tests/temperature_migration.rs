//! 2.1.22 输入温度迁移：保留历史 dump 的限制，不覆盖新字段。
use metatorio_data::{
    BoilerComponent, GeneratorComponent, adapt, store::PrototypeStore, types::FluidEnergySource,
};
use serde_json::json;

#[test]
fn legacy_temperature_limits_move_to_every_consumer_and_are_idempotent() {
    let cases = [
        ("boiler", "fluid_box", "input"),
        ("generator", "fluid_box", "input"),
        ("fusion-generator", "input_fluid_box", "input"),
        ("fusion-reactor", "input_fluid_box", "input"),
        ("mining-drill", "input_fluid_box", "input"),
        ("thruster", "fuel_fluid_box", "fuel"),
        ("thruster", "oxidizer_fluid_box", "oxidizer"),
    ];
    for (typename, box_name, suffix) in cases {
        let mut dump = json!({typename: {"consumer": {
            box_name: {"filter": "steam", "minimum_temperature": 100, "maximum_temperature": 500}
        }}});
        assert!(adapt::needs_adaptation(&dump), "{typename}/{box_name}");
        adapt::normalize_dump(&mut dump);
        let entry = &dump[typename]["consumer"];
        assert_eq!(entry[format!("minimum_{suffix}_temperature")], 100);
        assert_eq!(entry[format!("maximum_{suffix}_temperature")], 500);
        assert_eq!(entry[box_name], json!({"filter": "steam"}));
        assert!(!adapt::needs_adaptation(&dump));
        let before = dump.clone();
        adapt::normalize_dump(&mut dump);
        assert_eq!(dump, before);
    }
}

#[test]
fn new_limits_win_over_legacy_box_limits_and_both_load() {
    for legacy in [false, true] {
        let mut dump = json!({
            "fluid": {"steam": {"default_temperature": 15, "max_temperature": 1000}},
            "generator": {"turbine": {
                "fluid_box": {"filter": "steam"},
                "maximum_temperature": 165,
                "minimum_input_temperature": 165,
                "maximum_input_temperature": 500
            }},
            "boiler": {"heater": {
                "fluid_box": {"filter": "steam"},
                "minimum_input_temperature": 100,
                "maximum_input_temperature": 400
            }}
        });
        if legacy {
            for (typename, name) in [("generator", "turbine"), ("boiler", "heater")] {
                dump[typename][name]["fluid_box"]["minimum_temperature"] = json!(20);
                dump[typename][name]["fluid_box"]["maximum_temperature"] = json!(900);
            }
        }
        let original = dump.clone();
        let store = PrototypeStore::load(&dump).unwrap();
        assert_eq!(dump, original, "加载不得修改调用方的 dump");
        let generator = store
            .entity("turbine")
            .unwrap()
            .component::<GeneratorComponent>()
            .unwrap();
        assert_eq!(generator.minimum_input_temperature, Some(165.0));
        assert_eq!(generator.maximum_input_temperature, Some(500.0));
        let boiler = store
            .entity("heater")
            .unwrap()
            .component::<BoilerComponent>()
            .unwrap();
        assert_eq!(boiler.minimum_input_temperature, Some(100.0));
        assert_eq!(boiler.maximum_input_temperature, Some(400.0));
        assert_eq!(
            store.fluid_temperatures()["steam"],
            vec![15, 100, 165, 400, 500, 1000]
        );
    }
}

#[test]
fn fluid_energy_sources_migrate_bounds_and_unlimited_temperature() {
    for maximum in [None, Some(0), Some(800)] {
        let mut dump = json!({"assembling-machine": {"machine": {"energy_source": {
            "type": "fluid",
            "fluid_box": {"filter": "steam", "minimum_temperature": 100, "maximum_temperature": 500}
        }}}});
        if let Some(maximum) = maximum {
            dump["assembling-machine"]["machine"]["energy_source"]["maximum_temperature"] =
                json!(maximum);
        }
        assert!(adapt::needs_adaptation(&dump));
        adapt::normalize_dump(&mut dump);
        assert!(!adapt::needs_adaptation(&dump));
        let source: FluidEnergySource =
            serde_json::from_value(dump["assembling-machine"]["machine"]["energy_source"].clone())
                .unwrap();
        assert_eq!(source.minimum_input_temperature, Some(100.0));
        assert_eq!(source.maximum_input_temperature, Some(500.0));
        assert_eq!(
            source.maximum_temperature,
            maximum.filter(|m| *m != 0).map(f64::from)
        );
    }
    let dump = json!({"assembling-machine": {"machine": {"energy_source": {
        "type": "fluid", "maximum_temperature": 0
    }}}});
    assert!(
        adapt::needs_adaptation(&dump),
        "无旧流体盒字段也必须检测零哨兵值"
    );
    PrototypeStore::load(&dump).unwrap();
}
