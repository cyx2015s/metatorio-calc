//! 能量源 → 能源流（迁移自 metatorio-egui `energy_source_as_flow`）。
//!
//! 四种能量源：Electric（电力 + drain + 污染）、Heat（热 + 污染）、
//! Burner（物品燃料 + 燃尽产物 + 污染）、Fluid（烧流体 / 流体热源 + 污染）。
//!
//! 流体燃料/热源的温度相关项：`FuelSpec::Fluid(name, temperature)` 携带温度，
//! 展开层对温度区间两端各调一次并用 `TempFlow::add_dual` 合并（相关性表达）。

use crate::context::Context;
use crate::dual_var::DualVar;
use crate::id::IdWithQuality;
use crate::prim_var::Flow;
use metatorio_data::store::PrototypeGroup;
use metatorio_data::types::{Effect, EnergyAmount, EnergySource};

#[derive(Debug, Clone, PartialEq)]
pub struct FluidFuelSpec<'a> {
    pub fuel: &'a str,
    pub temperature: f64,
}

fn add(flow: &mut Flow, key: DualVar, value: f64) {
    if value != 0.0 {
        *flow.entry(key).or_insert(0.0) += value;
    }
}

/// 能量源 → 能源流（每秒）。
///
/// `fulfillment`：流体燃料/热源受最大流量限制时下调的产出因子（调用方乘到产出/速度上，
/// 初始值 1.0）。
pub fn energy_source_as_flow(
    ctx: &Context,
    energy_source: &EnergySource,
    energy_usage: EnergyAmount,
    effects: &Effect,
    fuel: Option<&FluidFuelSpec>,
    // 修改入参
    fulfillment: &mut f64,
) -> Flow {
    let mut map: Flow = Default::default();
    match energy_source {
        EnergySource::Void => {}
        EnergySource::Electric(source) => {
            let usage = energy_usage.amount * 60.0 * (1.0 + effects.consumption);
            add(&mut map, DualVar::Electricity, -usage);
            add(
                &mut map,
                DualVar::Electricity,
                -source
                    .drain
                    .as_ref()
                    .map(|d| d.amount * 60.0)
                    .unwrap_or_default(),
            );
            for (pollutant, emission) in &source.emissions_per_minute {
                add(
                    &mut map,
                    DualVar::Pollution {
                        name: pollutant.clone(),
                    },
                    emission * (1.0 + effects.pollution) * (1.0 + effects.consumption) / 60.0,
                );
            }
        }
        EnergySource::Heat(source) => {
            add(
                &mut map,
                DualVar::Heat,
                -energy_usage.amount * 60.0 * (1.0 + effects.consumption),
            );
            for (pollutant, emission) in &source.emissions_per_minute {
                add(
                    &mut map,
                    DualVar::Pollution {
                        name: pollutant.clone(),
                    },
                    emission * (1.0 + effects.pollution) * (1.0 + effects.consumption) / 60.0,
                );
            }
        }
        EnergySource::Burner(source) => {
            // 每秒能量消耗（燃烧效率折算）
            let usage =
                energy_usage.amount * 60.0 * (1.0 + effects.consumption) / source.effectivity;
            
                // 自动选择燃料：类别流（无燃尽产物，可隐式提升）
                add(
                    &mut map,
                    DualVar::ItemFuel {
                        category: source.fuel_categories.clone(),
                        has_burnt_result: false,
                    },
                    -usage,
                );
            
            for (pollutant, emission) in &source.emissions_per_minute {
                add(
                    &mut map,
                    DualVar::Pollution {
                        name: pollutant.clone(),
                    },
                    emission * (1.0 + effects.pollution) * (1.0 + effects.consumption) / 60.0,
                );
            }
        }
        EnergySource::Fluid(source) => {
            let usage =
                energy_usage.amount * 60.0 * (1.0 + effects.consumption) / source.effectivity;
            let filter = source.fluid_box.filter.clone().unwrap_or_default();
            if source.burns_fluid.unwrap_or(false) {
                // 烧流体作为燃料
                if let Some(FluidFuelSpec { fuel, temperature }) = fuel {
                    let Some(record) = ctx.prototype.get(PrototypeGroup::Fluid, *fuel) else {
                        return map;
                    };
                    let Some(fluid) =
                        record.component::<metatorio_data::generated_components::FluidComponent>()
                    else {
                        return map;
                    };
                    let fuel_value = fluid.fuel_value().amount;
                    let mut fuel_burn_speed = usage / fuel_value; // 每秒消耗的流体量
                    let max_flow = source.fluid_usage_per_tick * 60.0;
                    if fuel_burn_speed > max_flow && max_flow > 0.0 {
                        // 最大流量限制：产出按比例下调
                        *fulfillment = max_flow / fuel_burn_speed;
                        fuel_burn_speed = max_flow;
                    }
                    if fuel_burn_speed < max_flow && !source.scale_fluid_usage.unwrap_or(false) {
                        // 不可变流量：至少要满足指定流量
                        fuel_burn_speed = max_flow;
                    }
                    add(
                        &mut map,
                        DualVar::Fluid {
                            name: fuel.to_string(),
                            temperature: [*temperature as i32; 2],
                        },
                        -fuel_burn_speed,
                    );
                    
                }
            } else {
                // 流体热源（温度差发电/供热）
                if let Some(FluidFuelSpec { fuel, temperature }) = fuel {
                    let Some(record) = ctx.prototype.get(PrototypeGroup::Fluid, *fuel) else {
                        return map;
                    };
                    let Some(fluid) =
                        record.component::<metatorio_data::generated_components::FluidComponent>()
                    else {
                        return map;
                    };
                    let capacity = fluid.heat_capacity().amount;
                    let default_temperature = fluid.default_temperature;
                    let mut temperature_diff = *temperature - default_temperature;
                    if !source.scale_fluid_usage.unwrap_or(false)
                        && source.maximum_temperature > 0.0
                        && source.fluid_usage_per_tick == 0.0
                    {
                        temperature_diff = source.maximum_temperature - default_temperature;
                    }
                    let mut fuel_burn_speed = usage / capacity / temperature_diff;
                    let max_flow = source.fluid_usage_per_tick * 60.0;
                    if fuel_burn_speed > max_flow && max_flow > 0.0 {
                        *fulfillment = max_flow / fuel_burn_speed;
                        fuel_burn_speed = max_flow;
                    }
                    if fuel_burn_speed < max_flow && !source.scale_fluid_usage.unwrap_or(false) {
                        fuel_burn_speed = max_flow;
                    }
                    add(
                        &mut map,
                        DualVar::Fluid {
                            name: fuel.to_string(),
                            temperature: [*temperature as i32; 2],
                        },
                        -fuel_burn_speed,
                    );
                    add(
                        &mut map,
                        DualVar::FluidHeat {
                            filter: fuel.to_string(),
                        },
                        -fluid_heat(ctx, fuel, fuel_burn_speed, *temperature),
                    );
                } else {
                    // 自动选择热源流体：热量缺口
                    add(&mut map, DualVar::FluidHeat { filter }, -usage);
                }
            }
            for (pollutant, emission) in &source.emissions_per_minute {
                add(
                    &mut map,
                    DualVar::Pollution {
                        name: pollutant.clone(),
                    },
                    emission * (1.0 + effects.pollution) * (1.0 + effects.consumption) / 60.0,
                );
            }
        }
    }
    map
}

/// 流体热量：amount × (温度 − 默认温度) × 比热容（焦耳）。
fn fluid_heat(ctx: &Context, name: &str, amount: f64, temperature: f64) -> f64 {
    let Some(record) = ctx.prototype.get(PrototypeGroup::Fluid, name) else {
        return 0.0;
    };
    let Some(fluid) = record.component::<metatorio_data::generated_components::FluidComponent>()
    else {
        return 0.0;
    };
    amount * (temperature - fluid.default_temperature) * fluid.heat_capacity().amount
}
