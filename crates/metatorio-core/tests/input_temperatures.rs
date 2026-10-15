use metatorio_core::{
    context::{Context, GameState},
    dual_var::DualVar,
    energy::{FluidFuelSpec, FuelSpec, energy_source_as_flow},
};
use metatorio_data::{
    store::PrototypeStore,
    types::{Effect, EnergyAmount, EnergySource},
};
use serde_json::json;

#[test]
fn fluid_energy_sources_respect_input_limits_for_heat_and_fuel() {
    let store = PrototypeStore::load(&json!({"fluid": {"steam": {
        "default_temperature": 15, "max_temperature": 1000,
        "heat_capacity": "1kJ", "fuel_value": "1MJ"
    }}}))
    .unwrap();
    let game = GameState::default();
    let ctx = Context::new(&store, &game);
    for burns_fluid in [false, true] {
        let source: EnergySource = serde_json::from_value(json!({
            "type": "fluid", "burns_fluid": burns_fluid,
            "minimum_input_temperature": 100, "maximum_input_temperature": 500,
            "scale_fluid_usage": true
        }))
        .unwrap();
        for temperature in [99.0, 100.0, 500.0, 501.0] {
            let mut fulfillment = 1.0;
            let flow = energy_source_as_flow(
                &ctx,
                &source,
                EnergyAmount { amount: 1000.0 },
                &Effect::default(),
                Some(&FuelSpec::Fluid(FluidFuelSpec {
                    fuel: "steam",
                    temperature,
                })),
                false,
                &mut fulfillment,
            );
            if !burns_fluid && !(100.0..=500.0).contains(&temperature) {
                assert_eq!(fulfillment, 0.0);
                assert!(flow.is_empty());
            } else {
                assert_eq!(fulfillment, 1.0);
                let interval = if burns_fluid {
                    [100, 500]
                } else {
                    [temperature as i32; 2]
                };
                assert!(
                    flow[&DualVar::Fluid {
                        name: "steam".into(),
                        temperature: interval
                    }] < 0.0
                );
            }
        }
    }
}
