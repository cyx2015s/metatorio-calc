use std::collections::HashMap;

use indexmap::IndexMap;

use crate::{
    concept::Flow,
    factorio::{
        IdWithQuality,
        common::{Effect, EnergyAmount, EnergySource, index_map_update_entry},
        model::context::{FactorioContext, GenericItem},
    },
};

pub fn energy_source_as_flow(
    ctx: &FactorioContext,
    energy_source: &EnergySource,
    energy_usage: &EnergyAmount,
    effects: &Effect,
    instance_fuel: &Option<(String, i32)>,
    fulfillment: &mut f64,
) -> Flow<GenericItem> {
    let mut map = IndexMap::new();
    match energy_source {
        EnergySource::Electric(source) => {
            let energy_usage = energy_usage.amount * 60.0 * (1.0 + effects.consumption);
            index_map_update_entry(&mut map, GenericItem::Electricity, -energy_usage);
            index_map_update_entry(
                &mut map,
                GenericItem::Electricity,
                -source
                    .drain
                    .as_ref()
                    .map(|d| d.amount * 60.0)
                    .unwrap_or(energy_usage / 30.0),
            );
            for (pollutant, emmision) in source
                .emissions_per_minute
                .as_ref()
                .unwrap_or(&HashMap::new())
                .iter()
            {
                index_map_update_entry(
                    &mut map,
                    GenericItem::Pollution {
                        name: pollutant.clone(),
                    },
                    *emmision * (1.0 + effects.pollution) * (1.0 + effects.consumption) / 60.0,
                );
            }
        }
        EnergySource::Heat(source) => {
            index_map_update_entry(
                &mut map,
                GenericItem::Heat,
                -energy_usage.amount * 60.0 * (1.0 + effects.consumption),
            );
            for (pollutant, emmision) in source
                .emissions_per_minute
                .as_ref()
                .unwrap_or(&HashMap::new())
                .iter()
            {
                index_map_update_entry(
                    &mut map,
                    GenericItem::Pollution {
                        name: pollutant.clone(),
                    },
                    *emmision * (1.0 + effects.pollution) * (1.0 + effects.consumption) / 60.0,
                );
            }
        }
        EnergySource::Burner(source) => {
            let energy_usage =
                energy_usage.amount * 60.0 * (1.0 + effects.consumption) / source.effectivity; // 每秒的能量消耗
            if let Some(actual_fuel) = instance_fuel {
                // 使用具体燃料
                let fuel_prototype = ctx.items.get(&actual_fuel.0).expect("燃料在上下文中不存在");
                let fuel_property = fuel_prototype
                    .burn
                    .as_ref()
                    .expect("燃料在上下文中没有燃料值");
                let fuel_burn_speed = energy_usage / fuel_property.fuel_value.amount; // 一个物品的能量值

                index_map_update_entry(
                    &mut map,
                    GenericItem::Item(IdWithQuality(actual_fuel.0.clone(), actual_fuel.1 as u8)),
                    -fuel_burn_speed,
                );
                if let Some(burnt_result) = &fuel_property.burnt_result {
                    index_map_update_entry(
                        &mut map,
                        GenericItem::Item(IdWithQuality(burnt_result.clone(), actual_fuel.1 as u8)),
                        fuel_burn_speed,
                    );
                }
            } else {
                index_map_update_entry(
                    &mut map,
                    GenericItem::ItemFuel {
                        category: source.burner_usage.clone(),
                    },
                    -energy_usage,
                );
            }

            for (pollutant, emmision) in source
                .emissions_per_minute
                .as_ref()
                .unwrap_or(&HashMap::new())
                .iter()
            {
                index_map_update_entry(
                    &mut map,
                    GenericItem::Pollution {
                        name: pollutant.clone(),
                    },
                    *emmision * (1.0 + effects.pollution) * (1.0 + effects.consumption) / 60.0,
                );
            }
        }
        EnergySource::Fluid(source) => {
            // FIXME: 行为需要进一步确认
            let energy_usage =
                energy_usage.amount * 60.0 * (1.0 + effects.consumption) / source.effectivity; // 每秒的能量消耗
            if source.burns_fluid {
                if let Some(actual_fuel) = instance_fuel {
                    // 使用具体燃料
                    let fuel_prototype = ctx
                        .fluids
                        .get(&actual_fuel.0)
                        .expect("燃料在上下文中不存在");
                    let fuel_property = fuel_prototype
                        .fuel_value
                        .as_ref()
                        .expect("燃料在上下文中没有燃料值");
                    let mut fuel_burn_speed = energy_usage / fuel_property.amount; // 一个物品的能量值
                    if fuel_burn_speed > source.fluid_usage_per_tick * 60.0
                        && source.fluid_usage_per_tick > 0.0
                    {
                        // 设置最大值的情况下，无论如何都不能超过最大流量
                        *fulfillment = source.fluid_usage_per_tick * 60.0 / fuel_burn_speed;
                        fuel_burn_speed = source.fluid_usage_per_tick * 60.0;
                    }
                    if fuel_burn_speed < source.fluid_usage_per_tick * 60.0
                        && !source.scale_fluid_usage
                    {
                        // 如果没有设置成可变流量，则至少要满足指定流量
                        fuel_burn_speed = source.fluid_usage_per_tick * 60.0;
                    }

                    index_map_update_entry(
                        &mut map,
                        GenericItem::Fluid {
                            name: actual_fuel.0.clone(),
                            temperature: None,
                        },
                        -fuel_burn_speed,
                    );
                } else {
                    // 假定不会受到功率限制（流体热值太低且流量限制太小的情形）
                    index_map_update_entry(
                        &mut map,
                        GenericItem::FluidFuel {
                            filter: source.fluid_box.filter.clone(),
                        },
                        -energy_usage,
                    );
                }
                // 燃烧流体作为燃料
            } else {
                // 利用流体热能
                if let Some(actual_fuel) = instance_fuel {
                    // 使用具体燃料
                    let fuel_prototype = ctx
                        .fluids
                        .get(&actual_fuel.0)
                        .expect("燃料在上下文中不存在");
                    let fuel_property = fuel_prototype
                        .heat_capacity
                        .as_ref()
                        .expect("燃料在上下文中没有比热容");
                    let mut temperature_diff =
                        actual_fuel.1 as f64 - fuel_prototype.default_temperature;
                    if !source.scale_fluid_usage
                        && source.maximum_temperature > 0.0
                        && source.fluid_usage_per_tick == 0.0
                    {
                        temperature_diff =
                            source.maximum_temperature - fuel_prototype.default_temperature;
                    }
                    let mut fuel_burn_speed =
                        energy_usage / fuel_property.amount / temperature_diff;
                    if fuel_burn_speed > source.fluid_usage_per_tick * 60.0
                        && source.fluid_usage_per_tick > 0.0
                    {
                        // 设置最大值的情况下，无论如何都不能超过最大流量
                        *fulfillment = source.fluid_usage_per_tick * 60.0 / fuel_burn_speed;
                        fuel_burn_speed = source.fluid_usage_per_tick * 60.0;
                    }
                    if fuel_burn_speed < source.fluid_usage_per_tick * 60.0
                        && !source.scale_fluid_usage
                    {
                        // 如果没有设置成可变流量，则至少要满足指定流量
                        fuel_burn_speed = source.fluid_usage_per_tick * 60.0;
                    }

                    index_map_update_entry(
                        &mut map,
                        GenericItem::Fluid {
                            name: actual_fuel.0.clone(),
                            temperature: None,
                        },
                        -fuel_burn_speed,
                    );
                } else {
                    // 假定不会受到功率限制（流体热值太低且流量限制太小的情形）
                    index_map_update_entry(
                        &mut map,
                        GenericItem::FluidHeat {
                            filter: source.fluid_box.filter.clone(),
                        },
                        -energy_usage,
                    );
                }
            }

            for (pollutant, emmision) in source
                .emissions_per_minute
                .as_ref()
                .unwrap_or(&HashMap::new())
                .iter()
            {
                index_map_update_entry(
                    &mut map,
                    GenericItem::Pollution {
                        name: pollutant.clone(),
                    },
                    *emmision * (1.0 + effects.pollution) * (1.0 + effects.consumption) / 60.0,
                );
            }
        }
        EnergySource::Void(source) => {
            for (pollutant, emmision) in source
                .emissions_per_minute
                .as_ref()
                .unwrap_or(&HashMap::new())
                .iter()
            {
                index_map_update_entry(
                    &mut map,
                    GenericItem::Pollution {
                        name: pollutant.clone(),
                    },
                    *emmision * (1.0 + effects.pollution) * (1.0 + effects.consumption) / 60.0,
                );
            }
        }
    }
    map
}
