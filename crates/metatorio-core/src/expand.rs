//! 游戏机制到原始变量与流系数的展开。
//!
//! 一个配置可能对应多个原始变量：每个变温流体的可用温度都会形成一个
//! 温度决策，`TempFlow` 负责保留这些决策之间的相关性。所有流量均以
//! “每秒”计，负值表示消耗，正值表示产出。

use serde::{Deserialize, Serialize};

use crate::NORMAL_QUALITY;
use crate::context::Context;
use crate::dual_var::DualVar;
use crate::energy::{FluidFuelSpec, FuelSpec, ItemFuelSpec, energy_source_as_flow};
use crate::id::IdWithQuality;
use crate::mechanic::{
    BoilerMechanic, FluidFuelMechanic, FluidHeatMechanic, Fuel, GeneratorMechanic,
    ItemFuelMechanic, ItemLaunchMechanic, Mechanic, MiningMechanic, ModuleConfig, PlantMechanic,
    ReactorMechanic, RecipeMechanic, SolarMechanic, SpoilMechanic, quality_by_level,
};
use crate::prim_var::Expansion;
use crate::quality::calc_quality_distribution;
use crate::temp_flow::TempFlow;
use metatorio_data::store::{PrototypeGroup, PrototypeStore};
use metatorio_data::types::{
    BoilerMode, Effect, EffectType, EffectTypeLimitation, EnergyAmount, EnergySource, Ingredient,
    Product,
};
use metatorio_data::{
    AccumulatorComponent, BoilerComponent, CraftingMachineComponent, EntityComponent,
    GeneratorComponent, ItemComponent, MinableProperties, MiningDrillComponent, PlantComponent,
    ReactorComponent, RecipeComponent, ResourceEntityComponent, RocketSiloComponent,
    SolarPanelComponent,
};

/// Expand all mechanisms in the caller-provided order.
pub fn expand<'a, C: Clone>(
    mechanics: impl IntoIterator<Item = (C, &'a Mechanic)>,
    ctx: &Context,
) -> Expansion<C> {
    let mut expansion = Expansion::default();
    for (config, mechanic) in mechanics {
        let before = expansion.variables.len();
        match mechanic {
            Mechanic::Recipe(mechanic) => expand_recipe(config, mechanic, ctx, &mut expansion),
            Mechanic::Mining(mechanic) => expand_mining(config, mechanic, ctx, &mut expansion),
            Mechanic::Spoil(mechanic) => expand_spoil(config, mechanic, ctx, &mut expansion),
            Mechanic::Plant(mechanic) => expand_plant(config, mechanic, ctx, &mut expansion),
            Mechanic::ItemFuel(mechanic) => expand_item_fuel(config, mechanic, ctx, &mut expansion),
            Mechanic::ItemLaunch(mechanic) => {
                expand_item_launch(config, mechanic, ctx, &mut expansion)
            }
            Mechanic::Generator(mechanic) => {
                expand_generator(config, mechanic, ctx, &mut expansion)
            }
            Mechanic::Boiler(mechanic) => expand_boiler(config, mechanic, ctx, &mut expansion),
            Mechanic::Reactor(mechanic) => expand_reactor(config, mechanic, ctx, &mut expansion),
            Mechanic::Solar(mechanic) => expand_solar(config, mechanic, ctx, &mut expansion),
            Mechanic::FluidFuel(mechanic) => {
                expand_fluid_fuel(config, mechanic, ctx, &mut expansion)
            }
            Mechanic::FluidHeat(mechanic) => {
                expand_fluid_heat(config, mechanic, ctx, &mut expansion)
            }
        }
        // 给本次机制生成的每个变量写入单位成本。太阳能按表面倍率叠加所需
        // 蓄电器面积（instance_cost 拿不到表面太阳能系数，故在展开处计算）。
        let cost = if let Mechanic::Solar(mechanic) = mechanic {
            solar_instance_cost(ctx, mechanic)
        } else {
            instance_cost(ctx.prototype, mechanic)
        };
        for variable in &mut expansion.variables[before..] {
            variable.cost = cost;
        }
    }
    expansion
}

/// 实体碰撞箱面积（用于成本，复科旧实现）。
fn entity_area(store: &PrototypeStore, name: &str) -> Option<f64> {
    let bb = store
        .entity(name)?
        .component::<EntityComponent>()?
        .collision_box
        .as_ref()?;
    Some((bb.1.0 - bb.0.0).ceil().abs() * (bb.1.1 - bb.0.1).ceil().abs())
}

/// 单台实例成本（复刻旧实现 + 信标占地）：
/// - 带机器/设备的机制：机器碰撞箱面积 + Σ(信标面积 × 信标数 / 共享比例)
///   （缺失回退 16.0）；
/// - 变质：spoil_ticks / stack_size / 16；
/// - 太阳能：太阳能板面积（蓄电器面积按表面倍率在 `expand` 单独叠加）；
/// - 其余（种植/物品燃料/发射）：固定 16.0。
pub fn instance_cost(store: &PrototypeStore, mechanic: &Mechanic) -> f64 {
    let area = |name: &str| entity_area(store, name).unwrap_or(16.0);
    let beacon_area = |config: &ModuleConfig| -> f64 {
        config
            .beacons
            .iter()
            .map(|beacon| area(&beacon.beacon.id) * beacon.count as f64 / beacon.share.max(1.0))
            .sum()
    };
    match mechanic {
        Mechanic::Recipe(mechanic) => {
            area(&mechanic.machine.id) + beacon_area(&mechanic.module_config)
        }
        Mechanic::Mining(mechanic) => {
            area(&mechanic.machine.id) + beacon_area(&mechanic.module_config)
        }
        Mechanic::Generator(mechanic) => area(&mechanic.generator.id),
        Mechanic::Boiler(mechanic) => area(&mechanic.boiler.id),
        Mechanic::Reactor(mechanic) => area(&mechanic.reactor.id),
        Mechanic::Solar(mechanic) => area(&mechanic.solar_panel.id),
        Mechanic::Spoil(mechanic) => store
            .item(&mechanic.item.id)
            .and_then(|record| {
                let item = record.component::<ItemComponent>()?;
                Some(item.spoil_ticks? as f64 / item.stack_size.max(1) as f64 / 16.0)
            })
            .unwrap_or(16.0),
        // 流体燃料/流体热：转换机制，几乎无成本（复刻原版 cost()=0）。
        Mechanic::FluidFuel(_) | Mechanic::FluidHeat(_) => 0.0,
        _ => 16.0,
    }
}

/// 太阳能机制的单台成本：太阳能板面积 + 所需蓄电器面积。
/// 蓄电器数量依赖表面太阳能系数与昼夜周期（`solar_balance` 计算），故只能在
/// 展开处（有 `ctx`）求，`instance_cost` 拿不到表面系数。
fn solar_instance_cost(ctx: &Context, mechanic: &SolarMechanic) -> f64 {
    let panel_area = entity_area(ctx.prototype, &mechanic.solar_panel.id).unwrap_or(16.0);
    let accumulator_area = entity_area(ctx.prototype, &mechanic.accumulator.id).unwrap_or(16.0);
    let accumulators = solar_balance(ctx, mechanic)
        .map(|balance| balance.recommended_accumulators)
        .unwrap_or(0.0);
    panel_area + accumulators * accumulator_area
}

fn quality_name(ctx: &Context, level: usize) -> String {
    ctx.game
        .qualities
        .get(level)
        .cloned()
        .or_else(|| ctx.prototype.quality_order().get(level).cloned())
        .unwrap_or_else(|| NORMAL_QUALITY.to_string())
}

fn quality_level(ctx: &Context, name: &str) -> usize {
    ctx.game
        .qualities
        .iter()
        .position(|quality| quality == name)
        .unwrap_or(0)
}

fn quality_count(ctx: &Context) -> usize {
    ctx.prototype
        .quality_order()
        .len()
        .max(ctx.game.qualities.len())
}

fn quality_limit(ctx: &Context) -> usize {
    ctx.game
        .max_quality
        .min(quality_count(ctx).saturating_sub(1))
}

fn default_quality_multiplier(ctx: &Context, quality: &str) -> f64 {
    quality_by_level(ctx, quality_level(ctx, quality))
        .map(|quality| quality.default_multiplier())
        .unwrap_or(1.0)
}

fn bounded_quality(
    ctx: &Context,
    base: usize,
    change: i32,
    minimum: Option<&String>,
    maximum: Option<&String>,
) -> usize {
    let max = quality_limit(ctx);
    let mut level = (base as i32 + change).clamp(0, max as i32) as usize;
    if let Some(minimum) = minimum {
        level = level.max(quality_level(ctx, minimum));
    }
    if let Some(maximum) = maximum {
        level = level.min(quality_level(ctx, maximum));
    }
    level.min(max)
}

fn fluid_temperature(ctx: &Context, name: &str) -> f64 {
    ctx.prototype
        .get(PrototypeGroup::Fluid, name)
        .and_then(|record| record.component::<metatorio_data::FluidComponent>())
        .map(|fluid| fluid.default_temperature)
        .unwrap_or_default()
}

fn fluid_max_temperature(ctx: &Context, name: &str) -> f64 {
    ctx.prototype
        .get(PrototypeGroup::Fluid, name)
        .and_then(|record| record.component::<metatorio_data::FluidComponent>())
        .map(|fluid| fluid.max_temperature())
        .unwrap_or_default()
}

fn add_fluid_interval(temp: &mut TempFlow, name: &str, amount: f64, lower: f64, upper: f64) {
    temp.add(
        DualVar::Fluid {
            name: name.to_string(),
            temperature: [lower as i32, upper as i32],
        },
        amount,
    );
}

fn fluid_record<'a>(ctx: &'a Context, name: &str) -> Option<&'a metatorio_data::FluidComponent> {
    ctx.prototype
        .get(PrototypeGroup::Fluid, name)
        .and_then(|record| record.component())
}

fn item_record<'a>(ctx: &'a Context, name: &str) -> Option<&'a ItemComponent> {
    ctx.prototype
        .get(PrototypeGroup::Item, name)
        .and_then(|record| record.component())
}

fn entity_component<'a>(ctx: &'a Context, name: &str) -> Option<&'a EntityComponent> {
    ctx.prototype
        .entity(name)
        .and_then(|record| record.component())
}

/// 把明确燃料转换为 `FuelSpec`：`Fuel::Item` 用其自带品质，`Fuel::Fluid` 用
/// 名称 + 温度（None → 流体默认温度）。
fn explicit_fuel<'a>(ctx: &Context, fuel: Option<&'a Fuel>) -> Option<FuelSpec<'a>> {
    let fuel = fuel?;
    match fuel {
        Fuel::Item { item } => Some(FuelSpec::Item(ItemFuelSpec {
            name: item.id.as_str(),
            quality: item.quality.as_str(),
        })),
        Fuel::Fluid { fluid, temperature } => Some(FuelSpec::Fluid(FluidFuelSpec {
            fuel: fluid.as_str(),
            temperature: temperature
                .map(f64::from)
                .unwrap_or_else(|| fluid_temperature(ctx, fluid)),
        })),
    }
}

fn effective_energy_usage(usage: EnergyAmount, multiplier: f64) -> EnergyAmount {
    EnergyAmount {
        amount: usage.amount * multiplier,
    }
}

fn add_energy(
    ctx: &Context,
    temp: &mut TempFlow,
    source: &EnergySource,
    usage: EnergyAmount,
    effects: &Effect,
    fuel: Option<&FuelSpec<'_>>,
    fulfillment: &mut f64,
) {
    for (key, value) in energy_source_as_flow(ctx, source, usage, effects, fuel, fulfillment) {
        temp.add(key, value);
    }
}

fn add_machine_effects(effects: &mut Effect, receiver: Option<&metatorio_data::EffectReceiver>) {
    if let Some(receiver) = receiver {
        *effects = *effects + receiver.base_effect.unwrap_or_default();
    }
    *effects = effects.clamped();
}

fn restrict_effect(
    mut effect: Effect,
    machine_allowed: Option<&EffectTypeLimitation>,
    recipe: Option<&RecipeComponent>,
) -> Effect {
    let allowed = |kind: EffectType, recipe_allowed: bool| {
        recipe_allowed && machine_allowed.is_none_or(|limits| limits[kind])
    };
    if !allowed(
        EffectType::Consumption,
        recipe.is_none_or(|recipe| recipe.allow_consumption),
    ) {
        effect.consumption = effect.consumption.max(0.0);
    }
    if !allowed(
        EffectType::Speed,
        recipe.is_none_or(|recipe| recipe.allow_speed),
    ) {
        effect.speed = effect.speed.min(0.0);
    }
    if !allowed(
        EffectType::Productivity,
        recipe.is_none_or(|recipe| recipe.allow_productivity),
    ) {
        effect.productivity = 0.0;
    }
    if !allowed(
        EffectType::Pollution,
        recipe.is_none_or(|recipe| recipe.allow_pollution),
    ) {
        effect.pollution = effect.pollution.max(0.0);
    }
    if !allowed(
        EffectType::Quality,
        recipe.is_none_or(|recipe| recipe.allow_quality),
    ) {
        effect.quality = effect.quality.min(0.0);
    }
    effect
}

fn recipe_output_quality(
    ctx: &Context,
    base: usize,
    item: &metatorio_data::types::ItemProduct,
    level: usize,
) -> usize {
    if !item.affected_by_quality {
        return bounded_quality(
            ctx,
            base,
            i32::from(item.quality_change),
            item.quality_min.as_ref(),
            item.quality_max.as_ref(),
        );
    }
    bounded_quality(
        ctx,
        level,
        i32::from(item.quality_change),
        item.quality_min.as_ref(),
        item.quality_max.as_ref(),
    )
}

fn expand_recipe<C: Clone>(
    config: C,
    mechanic: &RecipeMechanic,
    ctx: &Context,
    out: &mut Expansion<C>,
) {
    let Some(recipe) = ctx
        .prototype
        .get(PrototypeGroup::Recipe, &mechanic.recipe.id)
        .and_then(|record| record.component::<RecipeComponent>())
    else {
        return;
    };
    let Some(crafter) = ctx.prototype.entity(&mechanic.machine.id) else {
        return;
    };
    let Some(machine) = crafter.component::<CraftingMachineComponent>() else {
        return;
    };
    let recipe_time = if recipe.energy_required > 0.0 {
        recipe.energy_required
    } else {
        0.5
    };

    let recipe_quality = quality_level(ctx, &mechanic.recipe.quality);
    let mut effects = restrict_effect(
        mechanic.module_config.get_effect(ctx),
        machine.allowed_effects.as_ref(),
        Some(recipe),
    );
    effects.productivity += ctx
        .game
        .recipe_productivity
        .get(&mechanic.recipe.id)
        .copied()
        .unwrap_or_default();
    effects.pollution += recipe.emissions_multiplier - 1.0;
    add_machine_effects(&mut effects, machine.effect_receiver.as_ref());

    let speed_multiplier = machine
        .crafting_speed_quality_multiplier
        .get(&mechanic.machine.quality)
        .copied()
        .or_else(|| {
            quality_by_level(ctx, quality_level(ctx, &mechanic.machine.quality))
                .map(|q| q.crafting_machine_speed_multiplier())
        })
        .unwrap_or(1.0);
    let energy_multiplier = if machine.quality_affects_energy_usage {
        quality_by_level(ctx, quality_level(ctx, &mechanic.machine.quality))
            .map(|quality| quality.crafting_machine_energy_usage_multiplier)
            .unwrap_or(1.0)
    } else {
        1.0
    };

    let mut base_speed = machine.crafting_speed * speed_multiplier / recipe_time;
    let mut fulfillment = 1.0;
    let fuel = explicit_fuel(ctx, mechanic.fuel.as_ref());
    let mut temp = TempFlow::new();
    add_energy(
        ctx,
        &mut temp,
        &machine.energy_source,
        effective_energy_usage(machine.energy_usage, energy_multiplier),
        &effects,
        fuel.as_ref(),
        &mut fulfillment,
    );
    if let EnergySource::Electric(source) = &machine.energy_source
        && source.drain.is_none()
    {
        temp.add(
            DualVar::Electricity,
            -machine.energy_usage.amount * energy_multiplier * 60.0 / 30.0,
        );
    }

    let module_consumption = mechanic.module_config.get_consumption(ctx);
    if module_consumption > 0.0 {
        temp.add(DualVar::Electricity, -module_consumption);
    }

    effects.productivity = effects.productivity.clamp(0.0, recipe.maximum_productivity);
    effects = effects.clamped();
    base_speed *= (1.0 + effects.speed).max(0.0);
    let scale = base_speed * fulfillment;

    // 组装机为 rocket-silo 时，配方额外产出火箭发射载荷（虚拟物品）：复刻旧实现
    // recipe.rs:587-601（Space Age 重量火箭 → RocketWeightCapacity；堆叠火箭 →
    // RocketSlotCapacity）。ItemLaunch 机制消耗该容量，二者在 LP 中配平。
    // 注意：火箭需 `rocket_parts_required` 次配方合成才完成，故每次合成的载荷
    // 应是整枚火箭容量 ÷ rocket_parts_required（旧版直接加满容量是错的）。
    if let Some(silo) = crafter.component::<RocketSiloComponent>() {
        let parts = silo.rocket_parts_required.max(1) as f64;
        if silo.launch_to_space_platforms {
            let lift_weight = if silo.lift_weight > 0.0 {
                silo.lift_weight
            } else {
                ctx.game.rocket_lift_weight
            };
            temp.add(DualVar::RocketWeightCapacity, lift_weight / parts);
        } else {
            let slots = silo
                .to_be_inserted_to_rocket_inventory_size
                .unwrap_or_default() as f64;
            temp.add(DualVar::RocketSlotCapacity, slots / parts);
        }
    }

    for ingredient in &recipe.ingredients {
        match ingredient {
            Ingredient::Item(item) => {
                let item_quality = bounded_quality(
                    ctx,
                    recipe_quality,
                    i32::from(item.quality_change),
                    item.quality_min.as_ref(),
                    item.quality_max.as_ref(),
                );
                temp.add(
                    DualVar::Item(IdWithQuality::new(
                        &item.name,
                        quality_name(ctx, item_quality),
                    )),
                    -f64::from(item.amount) * scale,
                );
            }
            Ingredient::Fluid(fluid) => {
                let default_temp = fluid_temperature(ctx, &fluid.name);
                let max_temp = fluid_max_temperature(ctx, &fluid.name);
                let lo = fluid
                    .temperature
                    .or(fluid.minimum_temperature)
                    .unwrap_or(default_temp);
                let hi = fluid
                    .temperature
                    .or(fluid.maximum_temperature)
                    .unwrap_or(max_temp);
                add_fluid_interval(&mut temp, &fluid.name, -fluid.amount * scale, lo, hi);
            }
        }
    }

    let quality_distribution =
        calc_quality_distribution(ctx, effects.quality, recipe_quality, quality_limit(ctx));
    for result in &recipe.results {
        match result {
            Product::Item(item) => {
                let output = item.normalized_output();
                let amount = (output.base + output.productivity * effects.productivity) * scale;
                for (level, probability) in quality_distribution.iter().copied().enumerate() {
                    if probability == 0.0 {
                        continue;
                    }
                    let output_quality = recipe_output_quality(ctx, recipe_quality, item, level);
                    temp.add(
                        DualVar::Item(IdWithQuality::new(
                            &item.name,
                            quality_name(ctx, output_quality),
                        )),
                        amount * probability,
                    );
                }
            }
            Product::Fluid(fluid) => {
                let temperature = fluid
                    .temperature
                    .unwrap_or_else(|| fluid_temperature(ctx, &fluid.name));
                let output = fluid.normalized_output();
                add_fluid_interval(
                    &mut temp,
                    &fluid.name,
                    (output.base + output.productivity * effects.productivity) * scale,
                    temperature,
                    temperature,
                );
            }
        }
    }
    out.variables.extend(temp.into_variables(config));
}

fn expand_mining<C: Clone>(
    config: C,
    mechanic: &MiningMechanic,
    ctx: &Context,
    out: &mut Expansion<C>,
) {
    let Some(machine_record) = ctx.prototype.entity(&mechanic.machine.id) else {
        return;
    };
    let Some(machine) = machine_record.component::<MiningDrillComponent>() else {
        return;
    };
    let Some(resource_record) = ctx.prototype.entity(&mechanic.resource) else {
        return;
    };
    let Some(resource) = resource_record.component::<EntityComponent>() else {
        return;
    };
    if let Some(resource_type) = resource_record.component::<ResourceEntityComponent>()
        && !machine.resource_categories.is_empty()
        && !machine
            .resource_categories
            .contains(&resource_type.category)
    {
        return;
    }
    let Some(minable) = resource.minable() else {
        return;
    };
    if minable.mining_time <= 0.0 {
        return;
    }

    let machine_quality = quality_level(ctx, &mechanic.machine.quality);
    let mut effects = restrict_effect(
        mechanic.module_config.get_effect(ctx),
        machine.allowed_effects.as_ref(),
        None,
    );
    effects.productivity += if machine.uses_force_mining_productivity_bonus {
        ctx.game.mining_productivity
    } else {
        0.0
    };
    add_machine_effects(&mut effects, machine.effect_receiver.as_ref());
    let drain_rate = machine.resource_drain_rate_percent.unwrap_or(100) as f64 / 100.0
        * quality_by_level(ctx, machine_quality)
            .map(|quality| quality.mining_drill_resource_drain_multiplier)
            .unwrap_or(1.0);

    let mut fulfillment = 1.0;
    let fuel = explicit_fuel(ctx, mechanic.fuel.as_ref());
    let mut temp = TempFlow::new();
    add_energy(
        ctx,
        &mut temp,
        &machine.energy_source,
        machine.energy_usage,
        &effects,
        fuel.as_ref(),
        &mut fulfillment,
    );
    if let EnergySource::Electric(source) = &machine.energy_source
        && source.drain.is_none()
    {
        temp.add(
            DualVar::Electricity,
            -machine.energy_usage.amount * 60.0 / 30.0,
        );
    }

    let base_speed = machine.mining_speed / minable.mining_time;
    let scale = base_speed * (1.0 + effects.speed).max(0.0) * fulfillment;
    temp.add(
        DualVar::Entity(IdWithQuality::new(&mechanic.resource, NORMAL_QUALITY)),
        -scale * drain_rate,
    );

    let module_consumption = mechanic.module_config.get_consumption(ctx);
    if module_consumption > 0.0 {
        temp.add(DualVar::Electricity, -module_consumption);
    }
    if let Some(fluid) = &minable.required_fluid {
        let amount = scale * minable.fluid_amount / 10.0;
        let default = fluid_temperature(ctx, fluid);
        add_fluid_interval(&mut temp, fluid, -amount, default, default);
    }

    let quality_distribution =
        calc_quality_distribution(ctx, effects.quality, 0, quality_limit(ctx));
    let add_item_output = |temp: &mut TempFlow, name: &str, amount: f64, distribution: &[f64]| {
        for (level, probability) in distribution.iter().copied().enumerate() {
            if probability > 0.0 {
                temp.add(
                    DualVar::Item(IdWithQuality::new(name, quality_name(ctx, level))),
                    amount * probability,
                );
            }
        }
    };

    if let Some(result) = &minable.result {
        let count = f64::from(minable.count.unwrap_or(1));
        add_item_output(
            &mut temp,
            result,
            scale * count * (1.0 + effects.productivity),
            &quality_distribution,
        );
    } else {
        for result in &minable.results {
            match result {
                Product::Item(item) => {
                    let output = item.normalized_output();
                    add_item_output(
                        &mut temp,
                        &item.name,
                        scale * (output.base + output.productivity * effects.productivity),
                        &quality_distribution,
                    );
                }
                Product::Fluid(fluid) => {
                    let output = fluid.normalized_output();
                    let temperature = fluid
                        .temperature
                        .unwrap_or_else(|| fluid_temperature(ctx, &fluid.name));
                    add_fluid_interval(
                        &mut temp,
                        &fluid.name,
                        scale * (output.base + output.productivity * effects.productivity),
                        temperature,
                        temperature,
                    );
                }
            }
        }
    }
    out.variables.extend(temp.into_variables(config));
}

fn expand_spoil<C: Clone>(
    config: C,
    mechanic: &SpoilMechanic,
    ctx: &Context,
    out: &mut Expansion<C>,
) {
    let Some(item) = item_record(ctx, &mechanic.item.id) else {
        return;
    };
    if item.spoil_ticks.unwrap_or_default() == 0 {
        return;
    }

    let base_quality = quality_level(ctx, &mechanic.item.quality);
    let spoil_rate = 60.0
        / (f64::from(item.spoil_ticks.unwrap_or_default())
            * quality_by_level(ctx, base_quality)
                .map(|quality| quality.spoil_ticks_multiplier())
                .unwrap_or(1.0));
    let output_quality = bounded_quality(
        ctx,
        base_quality,
        i32::from(item.spoil_quality_change.unwrap_or_default()),
        item.spoil_quality_min.as_ref(),
        item.spoil_quality_max.as_ref(),
    );
    let mut temp = TempFlow::new();
    temp.add(DualVar::Item(mechanic.item.clone()), -spoil_rate);
    if let Some(result) = &item.spoil_result {
        temp.add(
            DualVar::Item(IdWithQuality::new(
                result,
                quality_name(ctx, output_quality),
            )),
            spoil_rate,
        );
    }
    out.variables.extend(temp.into_variables(config));
}

fn expand_plant<C: Clone>(
    config: C,
    mechanic: &PlantMechanic,
    ctx: &Context,
    out: &mut Expansion<C>,
) {
    let Some(seed) = item_record(ctx, &mechanic.seed.id) else {
        return;
    };
    let Some(plant_name) = seed.plant_result.as_deref() else {
        return;
    };
    let Some(plant) = ctx
        .prototype
        .entity(plant_name)
        .and_then(|record| record.component::<PlantComponent>())
    else {
        return;
    };
    if plant.growth_ticks == 0 {
        return;
    }
    let rate = 60.0 / plant.growth_ticks as f64;
    let mut temp = TempFlow::new();
    temp.add(DualVar::Item(mechanic.seed.clone()), -rate);
    for (name, amount) in &plant.harvest_emissions {
        temp.add(DualVar::Pollution { name: name.clone() }, amount * rate);
    }

    if let Some(entity) = entity_component(ctx, plant_name)
        && let Some(minable) = entity.minable()
    {
        add_minable_outputs(&mut temp, ctx, minable, rate, NORMAL_QUALITY);
    }
    out.variables.extend(temp.into_variables(config));
}

fn add_minable_outputs(
    temp: &mut TempFlow,
    ctx: &Context,
    minable: &MinableProperties,
    scale: f64,
    quality: &str,
) {
    if let Some(result) = &minable.result {
        temp.add(
            DualVar::Item(IdWithQuality::new(result, quality)),
            scale * f64::from(minable.count.unwrap_or(1)),
        );
        return;
    }
    for result in &minable.results {
        match result {
            Product::Item(item) => temp.add(
                DualVar::Item(IdWithQuality::new(&item.name, quality)),
                scale * item.normalized_output().base,
            ),
            Product::Fluid(fluid) => {
                let temperature = fluid
                    .temperature
                    .unwrap_or_else(|| fluid_temperature(ctx, &fluid.name));
                add_fluid_interval(
                    temp,
                    &fluid.name,
                    scale * fluid.normalized_output().base,
                    temperature,
                    temperature,
                );
            }
        }
    }
}

fn expand_item_fuel<C: Clone>(
    config: C,
    mechanic: &ItemFuelMechanic,
    ctx: &Context,
    out: &mut Expansion<C>,
) {
    let Some(item) = item_record(ctx, &mechanic.item.id) else {
        return;
    };
    let fuel_value = item.fuel_value().amount;
    if fuel_value <= 0.0 || item.fuel_category.is_empty() {
        return;
    }
    let mut temp = TempFlow::new();
    temp.add(DualVar::Item(mechanic.item.clone()), -1.0);
    temp.add(
        DualVar::ItemFuel {
            category: vec![item.fuel_category.clone()],
            has_burnt_result: !item.burnt_result.is_empty(),
        },
        fuel_value,
    );
    if !item.burnt_result.is_empty() {
        temp.add(
            DualVar::Item(IdWithQuality::new(
                &item.burnt_result,
                mechanic.item.quality.clone(),
            )),
            1.0,
        );
    }
    out.variables.extend(temp.into_variables(config));
}

fn rocket_silo<'a>(ctx: &'a Context, weight_mode: bool) -> Option<&'a RocketSiloComponent> {
    ctx.prototype
        .group(PrototypeGroup::Entity)
        .find_map(|record| {
            let silo = record.component::<RocketSiloComponent>()?;
            (silo.launch_to_space_platforms == weight_mode).then_some(silo)
        })
}

fn expand_item_launch<C: Clone>(
    config: C,
    mechanic: &ItemLaunchMechanic,
    ctx: &Context,
    out: &mut Expansion<C>,
) {
    let Some(item) = item_record(ctx, &mechanic.item.id) else {
        return;
    };
    if item.rocket_launch_products.is_empty() {
        return;
    }
    let Some(silo) = rocket_silo(ctx, mechanic.weight_mode) else {
        return;
    };

    let (item_count, capacity, capacity_var) = if mechanic.weight_mode {
        let weight = item.weight.unwrap_or(ctx.game.default_item_weight);
        let lift_weight = if silo.lift_weight > 0.0 {
            silo.lift_weight
        } else {
            ctx.game.rocket_lift_weight
        };
        if weight <= 0.0 || lift_weight <= 0.0 {
            return;
        }
        (
            lift_weight / weight,
            lift_weight,
            DualVar::RocketWeightCapacity,
        )
    } else {
        let slots = silo
            .to_be_inserted_to_rocket_inventory_size
            .unwrap_or_default();
        if slots == 0 || item.stack_size == 0 {
            return;
        }
        (
            f64::from(slots) * f64::from(item.stack_size),
            f64::from(slots),
            DualVar::RocketSlotCapacity,
        )
    };

    let mut temp = TempFlow::new();
    temp.add(DualVar::Item(mechanic.item.clone()), -item_count);
    temp.add(capacity_var, -capacity);
    for product in &item.rocket_launch_products {
        temp.add(
            DualVar::Item(IdWithQuality::new(
                &product.name,
                mechanic.item.quality.clone(),
            )),
            product.normalized_output().base * item_count,
        );
    }
    out.variables.extend(temp.into_variables(config));
}

fn expand_generator<C: Clone>(
    config: C,
    mechanic: &GeneratorMechanic,
    ctx: &Context,
    out: &mut Expansion<C>,
) {
    let Some(generator) = ctx
        .prototype
        .entity(&mechanic.generator.id)
        .and_then(|record| record.component::<GeneratorComponent>())
    else {
        return;
    };
    let fluid_name = generator
        .fluid_box
        .filter
        .as_deref()
        .unwrap_or(&mechanic.fluid);
    let Some(fluid) = fluid_record(ctx, fluid_name) else {
        return;
    };
    let temperature = mechanic
        .temperature
        .map(f64::from)
        .unwrap_or(fluid.default_temperature);
    let output = generator.get_output(fluid_name, fluid, temperature);
    let mut temp = TempFlow::new();
    add_fluid_interval(
        &mut temp,
        fluid_name,
        -output.fluid_used_per_second,
        temperature,
        temperature,
    );
    temp.add(DualVar::Electricity, output.power_per_second);
    add_generator_spent_fluid(
        &mut temp,
        ctx,
        generator,
        fluid,
        output.fluid_used_per_second,
    );
    for (name, amount) in &generator.energy_source.emissions_per_minute {
        temp.add(DualVar::Pollution { name: name.clone() }, amount / 60.0);
    }
    temp.scale(default_quality_multiplier(ctx, &mechanic.generator.quality));
    out.variables.extend(temp.into_variables(config));
}

fn add_generator_spent_fluid(
    temp: &mut TempFlow,
    ctx: &Context,
    generator: &GeneratorComponent,
    input: &metatorio_data::FluidComponent,
    used: f64,
) {
    let spent = generator
        .spent_fluid
        .as_ref()
        .or(input.spent_fluid.as_ref());
    let Some(spent) = spent else {
        return;
    };
    if spent.amount <= 0.0 || spent.name.is_empty() {
        return;
    }
    let temperature = fluid_temperature(ctx, &spent.name);
    add_fluid_interval(
        temp,
        &spent.name,
        used * spent.amount,
        temperature,
        temperature,
    );
}

fn expand_boiler<C: Clone>(
    config: C,
    mechanic: &BoilerMechanic,
    ctx: &Context,
    out: &mut Expansion<C>,
) {
    let Some(boiler) = ctx
        .prototype
        .entity(&mechanic.boiler.id)
        .and_then(|record| record.component::<BoilerComponent>())
    else {
        return;
    };
    let input_name = boiler
        .fluid_box
        .filter
        .as_deref()
        .unwrap_or(&mechanic.fluid);
    let Some(input_fluid) = fluid_record(ctx, input_name) else {
        return;
    };
    let input_temperature = mechanic
        .temperature
        .map(f64::from)
        .unwrap_or(input_fluid.default_temperature);
    let fuel = explicit_fuel(ctx, mechanic.fuel.as_ref());
    let mut fulfillment = 1.0;
    let mut temp = TempFlow::new();
    add_energy(
        ctx,
        &mut temp,
        &boiler.energy_source,
        boiler.energy_consumption,
        &Effect::default(),
        fuel.as_ref(),
        &mut fulfillment,
    );

    match boiler.mode.unwrap_or(BoilerMode::HeatFluidInside) {
        BoilerMode::HeatFluidInside => {
            // No material transfer happens in this mode. The useful result is
            // heat added to the fluid already inside the input box.
            temp.add(
                DualVar::FluidHeat {
                    filter: input_name.to_string(),
                },
                boiler.energy_consumption.amount * 60.0 * fulfillment,
            );
        }
        BoilerMode::OutputToSeparatePipe => {
            let output_name = boiler
                .output_fluid_box
                .filter
                .as_deref()
                .unwrap_or(input_name);
            let Some(output_fluid) = fluid_record(ctx, output_name) else {
                return;
            };
            let Some(output) = boiler.heating_output(input_fluid, output_fluid, input_temperature)
            else {
                return;
            };
            add_fluid_interval(
                &mut temp,
                input_name,
                -output.input_amount_per_second * fulfillment,
                input_temperature,
                input_temperature,
            );
            add_fluid_interval(
                &mut temp,
                output_name,
                output.output_amount_per_second * fulfillment,
                output.output_temperature,
                output.output_temperature,
            );
        }
    }
    temp.scale(default_quality_multiplier(ctx, &mechanic.boiler.quality));
    out.variables.extend(temp.into_variables(config));
}

fn expand_solar<C: Clone>(
    config: C,
    mechanic: &SolarMechanic,
    ctx: &Context,
    out: &mut Expansion<C>,
) {
    let Some(panel) = ctx
        .prototype
        .entity(&mechanic.solar_panel.id)
        .and_then(|record| record.component::<SolarPanelComponent>())
    else {
        return;
    };
    let Some(accumulator) = ctx
        .prototype
        .entity(&mechanic.accumulator.id)
        .and_then(|record| record.component::<AccumulatorComponent>())
    else {
        return;
    };
    // 昼夜配平：Factorio 官方数据太阳能板平均出力为峰值的 0.7
    // （60 kW 峰值 → 42 kW 平均，含黄昏/黎明渐变），与 day_night_cycle
    // 长度无关（周期只缩放时间轴，不改变昼夜占比）。
    let day_fraction = 0.7;
    let performance =
        panel.performance_at_day * day_fraction + panel.performance_at_night * (1.0 - day_fraction);
    let coefficient = ctx.game.solar_power_multiplier.max(0.0);
    let production_per_second = panel.production.amount * 60.0 * coefficient * performance;
    if production_per_second <= 0.0 {
        return;
    }
    // 蓄电器用于吸收昼夜波动；平均稳定出力仍是周期平均太阳能功率。
    // 配平所需的周期溢出总电量由 solar_balance() 计算（机制面板展示）。
    let _capacity = accumulator
        .energy_source
        .buffer_capacity
        .map(|energy| energy.amount)
        .unwrap_or(0.0);
    let mut temp = TempFlow::new();
    temp.add(DualVar::Electricity, production_per_second);
    temp.scale(default_quality_multiplier(
        ctx,
        &mechanic.solar_panel.quality,
    ));
    out.variables.extend(temp.into_variables(config));
}

/// 太阳能配平信息（机制面板展示用，不参与 LP）。
///
/// 昼夜结构（[Factorio 官方默认](https://wiki.factorio.com/Game-day)，
/// 与实测 dawn=0.75/dusk=0.25/morning=0.55/evening=0.45 一致）：
/// 白天（满日照）50%、黄昏/黎明各 20%（性能线性渐变）、夜晚 10%。
/// sympy 精确积分（scripts/solar_balance_check.py）：
///   周期平均性能 = 7/10×performance_at_day + 3/10×performance_at_night
///   （默认 day=1/night=0 时为 0.7）
///   周期溢出总电量 = 21/125 × 峰值功率 × 周期秒 × (perf_day − perf_night)
///   即 0.168 × Δ。验证：60 kW × 420 s × 0.168 = 4.234 MJ ÷ 5 MJ ≈ 0.847，
///   与社区 25:21 ≈ 0.84 的配比吻合。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SolarBalance {
    /// 满日照峰值功率（J/s，含星球太阳能系数与品质倍率）。
    pub peak_power: f64,
    /// 周期平均稳定出力（J/s）= 峰值 × (7/10×perf_day + 3/10×perf_night)。
    pub average_power: f64,
    /// 一个昼夜周期的秒数。
    pub cycle_seconds: f64,
    /// 一个周期溢出的总电量（J）——蓄电器需要储存的能量。
    pub surplus_per_cycle: f64,
    /// 蓄电器容量（J）。
    pub accumulator_capacity: f64,
    /// 推荐蓄电器数量（每块面板）= surplus / capacity。
    pub recommended_accumulators: f64,
}

/// 计算太阳能机制的配平信息；面板/蓄电器原型缺失时返回 None。
pub fn solar_balance(ctx: &Context, mechanic: &SolarMechanic) -> Option<SolarBalance> {
    let panel = ctx
        .prototype
        .entity(&mechanic.solar_panel.id)
        .and_then(|record| record.component::<SolarPanelComponent>())?;
    let accumulator = ctx
        .prototype
        .entity(&mechanic.accumulator.id)
        .and_then(|record| record.component::<AccumulatorComponent>())?;
    let quality = default_quality_multiplier(ctx, &mechanic.solar_panel.quality);
    let coefficient = ctx.game.solar_power_multiplier.max(0.0);
    let peak = panel.production.amount * 60.0 * coefficient * quality;
    // 自定义性能曲线：昼夜各段按 performance 加权（不能直接用 0.7）。
    let perf_day = panel.performance_at_day;
    let perf_night = panel.performance_at_night;
    let average = peak * (0.7 * perf_day + 0.3 * perf_night);
    let cycle_seconds = ctx.game.day_night_cycle.max(1.0) / 60.0;
    // 蓄电器需储能量 = 21/125 × 峰值 × 周期 × |perf_day − perf_night|：
    // 无论白天还是夜晚性能更高，昼夜波动都需要储能（只取幅度）。
    let surplus = 0.168 * peak * cycle_seconds * (perf_day - perf_night).abs();
    let capacity = accumulator
        .energy_source
        .buffer_capacity
        .map(|energy| energy.amount)
        .unwrap_or(0.0);
    Some(SolarBalance {
        peak_power: peak,
        average_power: average,
        cycle_seconds,
        surplus_per_cycle: surplus,
        accumulator_capacity: capacity,
        recommended_accumulators: if capacity > 0.0 {
            surplus / capacity
        } else {
            0.0
        },
    })
}

fn expand_reactor<C: Clone>(
    config: C,
    mechanic: &ReactorMechanic,
    ctx: &Context,
    out: &mut Expansion<C>,
) {
    let Some(reactor) = ctx
        .prototype
        .entity(&mechanic.reactor.id)
        .and_then(|record| record.component::<ReactorComponent>())
    else {
        return;
    };
    if matches!(&reactor.energy_source, EnergySource::Heat(_)) {
        return;
    }
    let fuel = explicit_fuel(ctx, mechanic.fuel.as_ref());
    let mut temp = TempFlow::new();
    let mut fulfillment = 1.0;
    add_energy(
        ctx,
        &mut temp,
        &reactor.energy_source,
        reactor.consumption,
        &Effect::default(),
        fuel.as_ref(),
        &mut fulfillment,
    );

    let neighbour_multiplier = 1.0 + f64::from(mechanic.neighbours) * reactor.neighbour_bonus;
    let requested_heat = reactor.consumption.amount * 60.0 * neighbour_multiplier * fulfillment;
    let max_transfer = reactor.heat_buffer.max_transfer.amount * 60.0;
    let heat = if max_transfer > 0.0 {
        requested_heat.min(max_transfer)
    } else {
        requested_heat
    };
    temp.add(DualVar::Heat, heat);
    temp.scale(default_quality_multiplier(ctx, &mechanic.reactor.quality));
    out.variables.extend(temp.into_variables(config));
}

/// 流体燃料机制：燃烧 1 单位热值流体 → `fluid_value` 焦耳的流体燃料
/// 抽象能量（数值单位 J/s，与 energy.rs 一致；复刻原版
/// FluidFuelInstance::as_flow 的语义，去掉其 ×60 时间刻度）。
fn expand_fluid_fuel<C: Clone>(
    config: C,
    mechanic: &FluidFuelMechanic,
    ctx: &Context,
    out: &mut Expansion<C>,
) {
    let Some(fluid) = fluid_record(ctx, &mechanic.fluid) else {
        return;
    };
    let fuel_value = fluid.fuel_value().amount;
    if fuel_value <= 0.0 {
        return;
    }
    let temperature = mechanic
        .temperature
        .map(f64::from)
        .unwrap_or(fluid.default_temperature);
    let mut temp = TempFlow::new();
    add_fluid_interval(&mut temp, &mechanic.fluid, -1.0, temperature, temperature);
    temp.add(
        DualVar::FluidFuel {
            filter: mechanic.fluid.clone(),
        },
        fuel_value,
    );
    out.variables.extend(temp.into_variables(config));
}

/// 流体热量机制：把 1 单位高于默认温度的流体提取为
/// `heat_capacity × (温度 - 默认温度)` 焦耳的流体热量抽象能量
/// （数值单位 J/s，与 energy.rs 一致；复刻原版 FluidHeatInstance::as_flow
/// 的语义，去掉其 ×60 时间刻度；温度不高于默认温度时不产热）。
fn expand_fluid_heat<C: Clone>(
    config: C,
    mechanic: &FluidHeatMechanic,
    ctx: &Context,
    out: &mut Expansion<C>,
) {
    let Some(fluid) = fluid_record(ctx, &mechanic.fluid) else {
        return;
    };
    let temperature = mechanic
        .temperature
        .map(f64::from)
        .unwrap_or(fluid.default_temperature);
    if temperature <= fluid.default_temperature {
        return;
    }
    let heat_capacity = fluid.heat_capacity().amount;
    let mut temp = TempFlow::new();
    add_fluid_interval(&mut temp, &mechanic.fluid, -1.0, temperature, temperature);
    temp.add(
        DualVar::FluidHeat {
            filter: mechanic.fluid.clone(),
        },
        heat_capacity * (temperature - fluid.default_temperature),
    );
    out.variables.extend(temp.into_variables(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solar_expands_to_average_day_night_power() {
        // 太阳能板 60 kW 峰值（60000 W → 1000 J/tick）；蓄电器 5 MJ。
        let dump = serde_json::json!({
            "solar-panel": {
                "solar-panel": {
                    "name": "solar-panel",
                    "production": "60kW",
                    "performance_at_day": 1.0,
                    "performance_at_night": 0.0
                }
            },
            "accumulator": {
                "accumulator": {
                    "name": "accumulator",
                    "energy_source": { "type": "electric", "buffer_capacity": "5MJ" }
                }
            },
            "quality": {
                "normal": { "name": "normal", "level": 0 }
            }
        });
        let store = metatorio_data::store::PrototypeStore::load(&dump).expect("dump 加载失败");
        let game = crate::context::GameState {
            qualities: vec!["normal".to_string()],
            max_quality: 0,
            ..Default::default()
        };
        let ctx = Context::new(&store, &game);
        let mechanic = Mechanic::Solar(SolarMechanic {
            solar_panel: IdWithQuality::new("solar-panel", "normal"),
            accumulator: IdWithQuality::new("accumulator", "normal"),
        });
        let expansion = expand([(0u32, &mechanic)], &ctx);
        assert_eq!(expansion.variables.len(), 1);
        let electricity = expansion.variables[0]
            .flow
            .get(&DualVar::Electricity)
            .copied()
            .expect("太阳能展开应产出电力");
        // 60 kW 峰值 × 0.7 平均 = 42 kW = 42000 J/s
        assert!(
            (electricity - 42000.0).abs() < 1e-6,
            "electricity = {electricity}"
        );
    }

    #[test]
    fn solar_uses_planet_solar_multiplier() {
        let dump = serde_json::json!({
            "solar-panel": {
                "solar-panel": {
                    "name": "solar-panel",
                    "production": "60kW"
                }
            },
            "accumulator": {
                "accumulator": {
                    "name": "accumulator",
                    "energy_source": { "type": "electric", "buffer_capacity": "5MJ" }
                }
            },
            "quality": {
                "normal": { "name": "normal", "level": 0 }
            }
        });
        let store = metatorio_data::store::PrototypeStore::load(&dump).expect("dump 加载失败");
        // 模拟一颗太阳能系数 0.5 的星球（如昏暗地表）。
        let game = crate::context::GameState {
            qualities: vec!["normal".to_string()],
            max_quality: 0,
            solar_power_multiplier: 0.5,
            ..Default::default()
        };
        let ctx = Context::new(&store, &game);
        let mechanic = Mechanic::Solar(SolarMechanic {
            solar_panel: IdWithQuality::new("solar-panel", "normal"),
            accumulator: IdWithQuality::new("accumulator", "normal"),
        });
        let expansion = expand([(0u32, &mechanic)], &ctx);
        let electricity = expansion.variables[0]
            .flow
            .get(&DualVar::Electricity)
            .copied()
            .expect("太阳能展开应产出电力");
        // 60 kW × 0.7 × 0.5 = 21 kW = 21000 J/s
        assert!(
            (electricity - 21000.0).abs() < 1e-6,
            "electricity = {electricity}"
        );
    }

    /// 太阳能单位成本 = 面板面积 + 所需蓄电器面积（依赖表面系数；不同表面不同）。
    #[test]
    fn solar_cost_includes_accumulators_and_follows_surface() {
        let dump = serde_json::json!({
            "solar-panel": {
                "solar-panel": {
                    "name": "solar-panel",
                    "production": "60kW",
                    "collision_box": [[-1, -1], [1, 1]]
                }
            },
            "accumulator": {
                "accumulator": {
                    "name": "accumulator",
                    "energy_source": { "type": "electric", "buffer_capacity": "5MJ" },
                    "collision_box": [[-0.5, -0.5], [0.5, 0.5]]
                }
            },
            "quality": {
                "normal": { "name": "normal", "level": 0 }
            }
        });
        let store = metatorio_data::store::PrototypeStore::load(&dump).expect("dump 加载失败");
        let sm = SolarMechanic {
            solar_panel: IdWithQuality::new("solar-panel", "normal"),
            accumulator: IdWithQuality::new("accumulator", "normal"),
        };
        let mechanic = Mechanic::Solar(sm.clone());
        let cost_at = |coefficient: f64| {
            let game = crate::context::GameState {
                qualities: vec!["normal".to_string()],
                max_quality: 0,
                solar_power_multiplier: coefficient,
                ..Default::default()
            };
            let ctx = Context::new(&store, &game);
            let expansion = expand([(0u32, &mechanic)], &ctx);
            let balance = solar_balance(&ctx, &sm).expect("配平应可计算");
            (
                expansion.variables[0].cost,
                balance.recommended_accumulators,
            )
        };
        // 面板面积 = 2×2 = 4，蓄电器面积 = 1×1 = 1；成本 = 面板面积 + 蓄电器数×蓄电器面积。
        let (c1, acc1) = cost_at(1.0);
        let expected1 = 4.0 + acc1 * 1.0;
        assert!(
            (c1 - expected1).abs() < 1e-6,
            "c1={c1} expected1={expected1}"
        );
        assert!(c1 > 4.0, "太阳能成本应含蓄电器面积（大于面板面积 4）");
        // 表面系数更低 → 每面板蓄电器需求更少（周期盈余随峰值同比缩小）→ 成本不同。
        let (c05, acc05) = cost_at(0.5);
        assert!(
            acc05 < acc1,
            "低倍率下每面板蓄电器需求应更少: {acc05} vs {acc1}"
        );
        assert!((c05 - (4.0 + acc05)).abs() < 1e-6, "c05={c05}");
        assert!((c05 - c1).abs() > 1e-6, "不同表面系数下太阳能成本应不同");
    }

    #[test]
    fn solar_balance_matches_community_accumulator_ratio() {
        let dump = serde_json::json!({
            "solar-panel": {
                "solar-panel": {
                    "name": "solar-panel",
                    "production": "60kW"
                }
            },
            "accumulator": {
                "accumulator": {
                    "name": "accumulator",
                    "energy_source": { "type": "electric", "buffer_capacity": "5MJ" }
                }
            },
            "quality": {
                "normal": { "name": "normal", "level": 0 }
            }
        });
        let store = metatorio_data::store::PrototypeStore::load(&dump).expect("dump 加载失败");
        let game = crate::context::GameState {
            qualities: vec!["normal".to_string()],
            max_quality: 0,
            ..Default::default()
        };
        let ctx = Context::new(&store, &game);
        let mechanic = SolarMechanic {
            solar_panel: IdWithQuality::new("solar-panel", "normal"),
            accumulator: IdWithQuality::new("accumulator", "normal"),
        };
        let balance = solar_balance(&ctx, &mechanic).expect("配平信息应可计算");
        // 峰值 60 kW，平均 42 kW（默认 day=1/night=0 → 0.7）。
        assert!((balance.peak_power - 60000.0).abs() < 1e-6);
        assert!((balance.average_power - 42000.0).abs() < 1e-6);
        // 默认昼夜周期 25200 ticks = 420 s。
        assert!((balance.cycle_seconds - 420.0).abs() < 1e-9);
        // 周期溢出总电量 = 21/125 × 峰值 × 周期 = 4.2336 MJ。
        assert!((balance.surplus_per_cycle - 4_233_600.0).abs() < 1e-3);
        // 蓄电器 5 MJ → 每块面板约 0.847 个（社区标准 0.84）。
        assert!((balance.recommended_accumulators - 0.847).abs() < 0.01);
        assert!((balance.accumulator_capacity - 5_000_000.0).abs() < 1e-6);
    }

    #[test]
    fn solar_balance_respects_custom_performance_curve() {
        // 自定义原型：夜间仍保持 50% 出力（performance_at_night = 0.5）。
        let dump = serde_json::json!({
            "solar-panel": {
                "custom-solar-panel": {
                    "name": "custom-solar-panel",
                    "production": "60kW",
                    "performance_at_day": 1.0,
                    "performance_at_night": 0.5
                }
            },
            "accumulator": {
                "accumulator": {
                    "name": "accumulator",
                    "energy_source": { "type": "electric", "buffer_capacity": "5MJ" }
                }
            },
            "quality": {
                "normal": { "name": "normal", "level": 0 }
            }
        });
        let store = metatorio_data::store::PrototypeStore::load(&dump).expect("dump 加载失败");
        let game = crate::context::GameState {
            qualities: vec!["normal".to_string()],
            max_quality: 0,
            ..Default::default()
        };
        let ctx = Context::new(&store, &game);
        let mechanic = SolarMechanic {
            solar_panel: IdWithQuality::new("custom-solar-panel", "normal"),
            accumulator: IdWithQuality::new("accumulator", "normal"),
        };
        let balance = solar_balance(&ctx, &mechanic).expect("配平信息应可计算");
        // 平均 = 峰值 × (0.7×1 + 0.3×0.5) = 0.85 × 60 kW = 51 kW。
        assert!((balance.average_power - 51000.0).abs() < 1e-6);
        // 盈余 = 21/125 × 60000 × 420 × (1 − 0.5) = 2.1168 MJ。
        assert!((balance.surplus_per_cycle - 2_116_800.0).abs() < 1e-3);
        // 推荐蓄电器数减半。
        assert!((balance.recommended_accumulators - 0.4234).abs() < 0.01);
    }

    #[test]
    fn solar_balance_handles_night_stronger_than_day() {
        // 反向性能曲线（performance_at_night > performance_at_day）：
        // 平均按加权仍正确，蓄电器储能只取波动幅度（|Δ|）。
        let dump = serde_json::json!({
            "solar-panel": {
                "night-solar-panel": {
                    "name": "night-solar-panel",
                    "production": "60kW",
                    "performance_at_day": 0.4,
                    "performance_at_night": 1.0
                }
            },
            "accumulator": {
                "accumulator": {
                    "name": "accumulator",
                    "energy_source": { "type": "electric", "buffer_capacity": "5MJ" }
                }
            },
            "quality": {
                "normal": { "name": "normal", "level": 0 }
            }
        });
        let store = metatorio_data::store::PrototypeStore::load(&dump).expect("dump 加载失败");
        let game = crate::context::GameState {
            qualities: vec!["normal".to_string()],
            max_quality: 0,
            ..Default::default()
        };
        let ctx = Context::new(&store, &game);
        let mechanic = SolarMechanic {
            solar_panel: IdWithQuality::new("night-solar-panel", "normal"),
            accumulator: IdWithQuality::new("accumulator", "normal"),
        };
        let balance = solar_balance(&ctx, &mechanic).expect("配平信息应可计算");
        // 平均 = 峰值 × (0.7×0.4 + 0.3×1.0) = 0.58 × 60 kW = 34.8 kW。
        assert!((balance.average_power - 34800.0).abs() < 1e-6);
        // 盈余 = 21/125 × 60000 × 420 × |0.4 − 1.0| = 2.54016 MJ。
        assert!((balance.surplus_per_cycle - 2_540_160.0).abs() < 1e-3);
        assert!(balance.surplus_per_cycle > 0.0, "波动幅度应产生正储能需求");
    }

    #[test]
    fn solar_expands_with_custom_performance_curve() {
        let dump = serde_json::json!({
            "solar-panel": {
                "custom-solar-panel": {
                    "name": "custom-solar-panel",
                    "production": "60kW",
                    "performance_at_day": 1.0,
                    "performance_at_night": 0.5
                }
            },
            "accumulator": {
                "accumulator": {
                    "name": "accumulator",
                    "energy_source": { "type": "electric", "buffer_capacity": "5MJ" }
                }
            },
            "quality": {
                "normal": { "name": "normal", "level": 0 }
            }
        });
        let store = metatorio_data::store::PrototypeStore::load(&dump).expect("dump 加载失败");
        let game = crate::context::GameState {
            qualities: vec!["normal".to_string()],
            max_quality: 0,
            ..Default::default()
        };
        let ctx = Context::new(&store, &game);
        let mechanic = Mechanic::Solar(SolarMechanic {
            solar_panel: IdWithQuality::new("custom-solar-panel", "normal"),
            accumulator: IdWithQuality::new("accumulator", "normal"),
        });
        let expansion = expand([(0u32, &mechanic)], &ctx);
        let electricity = expansion.variables[0]
            .flow
            .get(&DualVar::Electricity)
            .copied()
            .expect("太阳能展开应产出电力");
        // 60 kW × (0.7×1 + 0.3×0.5) = 51 kW = 51000 J/s。
        assert!(
            (electricity - 51000.0).abs() < 1e-6,
            "electricity = {electricity}"
        );
    }

    #[test]
    fn parallel_variants_are_caller_defined() {
        let mut flow = TempFlow::new();
        let mut low = crate::prim_var::Flow::default();
        low.insert(
            DualVar::Fluid {
                name: "water".to_string(),
                temperature: [15, 165],
            },
            2.0,
        );
        let mut high = crate::prim_var::Flow::default();
        high.insert(
            DualVar::Fluid {
                name: "water".to_string(),
                temperature: [165, 500],
            },
            2.0,
        );
        flow.add_parallel([low, high]);
        let variables = flow.into_variables("test");
        assert_eq!(variables.len(), 2);
        assert!(variables[0].flow.contains_key(&DualVar::Fluid {
            name: "water".to_string(),
            temperature: [15, 165],
        }));
        assert!(variables[1].flow.contains_key(&DualVar::Fluid {
            name: "water".to_string(),
            temperature: [165, 500],
        }));
    }
}
