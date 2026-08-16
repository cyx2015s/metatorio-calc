//! 能量源到流的转换。
//!
//! 这里仅计算一秒内的能量与污染流。机器的运行速度由调用方计算，
//! `fulfillment` 用来表达流体能量源的流量上限对机器速度的限制。

use crate::context::Context;
use crate::dual_var::DualVar;
use crate::prim_var::Flow;
use metatorio_data::generated_components::{FluidComponent, ItemComponent};
use metatorio_data::store::PrototypeGroup;
use metatorio_data::types::{Effect, EnergyAmount, EnergySource};

/// 明确选择的燃料。
///
/// 未明确选择燃料时，能量源会生成 `ItemFuel` 或 `FluidHeat` 筛选流，
/// 由求解器从其它机制提供的燃料中选择可行项。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FuelSpec<'a> {
    Item(ItemFuelSpec<'a>),
    Fluid(FluidFuelSpec<'a>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemFuelSpec<'a> {
    pub name: &'a str,
    pub quality: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FluidFuelSpec<'a> {
    pub fuel: &'a str,
    pub temperature: f64,
}

fn add(flow: &mut Flow, key: DualVar, value: f64) {
    if value == 0.0 {
        return;
    }
    let new_value = flow.get(&key).copied().unwrap_or_default() + value;
    if new_value == 0.0 {
        flow.shift_remove(&key);
    } else {
        flow.insert(key, new_value);
    }
}

fn effective_effectivity(effectivity: f64) -> f64 {
    if effectivity > 0.0 { effectivity } else { 1.0 }
}

fn effective_scale_fluid_usage(scale_fluid_usage: Option<bool>) -> bool {
    scale_fluid_usage.unwrap_or(false)
}

fn fluid_record<'a>(ctx: &'a Context, name: &str) -> Option<&'a FluidComponent> {
    ctx.prototype
        .get(PrototypeGroup::Fluid, name)
        .and_then(|record| record.component::<FluidComponent>())
}

fn item_record<'a>(ctx: &'a Context, name: &str) -> Option<&'a ItemComponent> {
    ctx.prototype
        .get(PrototypeGroup::Item, name)
        .and_then(|record| record.component::<ItemComponent>())
}

fn add_emissions(
    flow: &mut Flow,
    emissions_per_minute: &std::collections::BTreeMap<String, f64>,
    effects: &Effect,
    fuel_multiplier: f64,
) {
    let multiplier = (1.0 + effects.pollution) * (1.0 + effects.consumption);
    for (name, amount) in emissions_per_minute {
        add(
            flow,
            DualVar::Pollution { name: name.clone() },
            amount * multiplier * fuel_multiplier / 60.0,
        );
    }
}

/// Apply the fluid source's per-tick usage rule.
fn fluid_usage(requested: f64, maximum: f64, scale_usage: bool, fulfillment: &mut f64) -> f64 {
    if requested <= 0.0 {
        return 0.0;
    }
    if maximum <= 0.0 {
        return requested;
    }

    let mut used = requested;
    if used > maximum {
        *fulfillment = (*fulfillment).min(maximum / used);
        used = maximum;
    } else if !scale_usage {
        // A fixed fluid usage keeps the source at its declared flow rate.
        used = maximum;
    }
    used
}

/// Convert an energy source to one-second flow coefficients.
pub fn energy_source_as_flow(
    ctx: &Context,
    energy_source: &EnergySource,
    energy_usage: EnergyAmount,
    effects: &Effect,
    fuel: Option<&FuelSpec<'_>>,
    fulfillment: &mut f64,
) -> Flow {
    let mut flow = Flow::default();
    let usage = energy_usage.amount * 60.0 * (1.0 + effects.consumption);

    match energy_source {
        EnergySource::Void => {}
        EnergySource::Electric(source) => {
            add(&mut flow, DualVar::Electricity, -usage);
            if let Some(drain) = source.drain {
                add(&mut flow, DualVar::Electricity, -drain.amount * 60.0);
            }
            add_emissions(&mut flow, &source.emissions_per_minute, effects, 1.0);
        }
        EnergySource::Heat(source) => {
            add(&mut flow, DualVar::Heat, -usage);
            add_emissions(&mut flow, &source.emissions_per_minute, effects, 1.0);
        }
        EnergySource::Burner(source) => {
            let usage = usage / effective_effectivity(source.effectivity);
            let mut fuel_emissions_multiplier = 1.0;

            match fuel {
                Some(FuelSpec::Item(item_spec)) => {
                    let Some(item) = item_record(ctx, item_spec.name) else {
                        return flow;
                    };
                    let fuel_value = item.fuel_value().amount;
                    if fuel_value <= 0.0 {
                        return flow;
                    }
                    let burn_rate = usage / fuel_value;
                    add(
                        &mut flow,
                        DualVar::Item(crate::id::IdWithQuality::new(
                            item_spec.name,
                            item_spec.quality,
                        )),
                        -burn_rate,
                    );
                    if !item.burnt_result.is_empty() {
                        add(
                            &mut flow,
                            DualVar::Item(crate::id::IdWithQuality::new(
                                item.burnt_result.clone(),
                                item_spec.quality,
                            )),
                            burn_rate,
                        );
                    }
                    fuel_emissions_multiplier = item.fuel_emissions_multiplier;
                }
                Some(FuelSpec::Fluid(_)) => {
                    // A burner cannot consume a fluid fuel. Keep the mechanism
                    // usable when a shared config contains an irrelevant fuel.
                    return flow;
                }
                None => {
                    add(
                        &mut flow,
                        DualVar::ItemFuel {
                            category: source.fuel_categories.clone(),
                            has_burnt_result: source
                                .burnt_inventory_size
                                .is_some_and(|size| size > 0),
                        },
                        -usage,
                    );
                }
            }

            add_emissions(
                &mut flow,
                &source.emissions_per_minute,
                effects,
                fuel_emissions_multiplier,
            );
        }
        EnergySource::Fluid(source) => {
            let usage = usage / effective_effectivity(source.effectivity);
            let maximum = source.fluid_usage_per_tick * 60.0;
            let scale_usage = effective_scale_fluid_usage(source.scale_fluid_usage);
            let burns_fluid = source.burns_fluid.unwrap_or(false);

            match fuel {
                Some(FuelSpec::Fluid(spec)) => {
                    let Some(fluid) = fluid_record(ctx, spec.fuel) else {
                        return flow;
                    };
                    let (requested, fuel_emissions_multiplier) = if burns_fluid {
                        let fuel_value = fluid.fuel_value().amount;
                        if fuel_value <= 0.0 {
                            return flow;
                        }
                        (usage / fuel_value, fluid.emissions_multiplier)
                    } else {
                        let temperature_difference = spec.temperature - fluid.default_temperature;
                        if temperature_difference <= 0.0 {
                            *fulfillment = 0.0;
                            return flow;
                        }
                        (
                            usage / fluid.heat_capacity().amount / temperature_difference,
                            fluid.emissions_multiplier,
                        )
                    };
                    let used = fluid_usage(requested, maximum, scale_usage, fulfillment);
                    add(
                        &mut flow,
                        DualVar::Fluid {
                            name: spec.fuel.to_string(),
                            temperature: [spec.temperature as i32; 2],
                        },
                        -used,
                    );
                    if !burns_fluid {
                        add(
                            &mut flow,
                            DualVar::FluidHeat {
                                filter: spec.fuel.to_string(),
                            },
                            -used
                                * (spec.temperature - fluid.default_temperature)
                                * fluid.heat_capacity().amount,
                        );
                    }
                    add_spent_fluid(ctx, &mut flow, &source.spent_fluid, used);
                    add_emissions(
                        &mut flow,
                        &source.emissions_per_minute,
                        effects,
                        fuel_emissions_multiplier,
                    );
                }
                Some(FuelSpec::Item(_)) => {}
                None => {
                    // With no selected fluid, the solver only sees the energy
                    // deficit. A concrete provider (流体燃料机制/锅炉等) supplies
                    // the matching abstract flow in a later conversion step.
                    let filter = source.fluid_box.filter.clone().unwrap_or_default();
                    if burns_fluid {
                        // 燃烧流体：消耗"流体燃料"抽象能量（热值流体被燃烧）。
                        add(&mut flow, DualVar::FluidFuel { filter }, -usage);
                    } else {
                        // 利用流体热源：消耗"流体热量"抽象能量。
                        add(&mut flow, DualVar::FluidHeat { filter }, -usage);
                    }
                    add_emissions(&mut flow, &source.emissions_per_minute, effects, 1.0);
                }
            }
        }
    }
    flow
}

fn add_spent_fluid(
    ctx: &Context,
    flow: &mut Flow,
    spent: &metatorio_data::generated_components::SpentFluidSpecification,
    amount: f64,
) {
    if spent.amount <= 0.0 || spent.name.is_empty() || amount <= 0.0 {
        return;
    }
    let temperature = fluid_record(ctx, &spent.name)
        .map(|fluid| fluid.default_temperature as i32)
        .unwrap_or_default();
    add(
        flow,
        DualVar::Fluid {
            name: spent.name.clone(),
            temperature: [temperature; 2],
        },
        amount * spent.amount,
    );
}
