//! 游戏机制到原始变量与流系数的展开。
//!
//! 一个配置可能对应多个原始变量：每个变温流体的可用温度都会形成一个
//! 温度决策，`TempFlow` 负责保留这些决策之间的相关性。所有流量均以
//! “每秒”计，负值表示消耗，正值表示产出。

use crate::NORMAL_QUALITY;
use crate::context::Context;
use crate::dual_var::DualVar;
use crate::energy::{FluidFuelSpec, FuelSpec, ItemFuelSpec, energy_source_as_flow};
use crate::id::IdWithQuality;
use crate::mechanic::{
    BoilerMechanic, GeneratorMechanic, ItemFuelMechanic, ItemLaunchMechanic, Mechanic,
    MiningMechanic, PlantMechanic, ReactorMechanic, RecipeMechanic, SpoilMechanic,
    quality_by_level,
};
use crate::prim_var::Expansion;
use crate::quality::calc_quality_distribution;
use crate::temp_flow::TempFlow;
use metatorio_data::generated_components::{
    BoilerComponent, CraftingMachineComponent, EntityComponent, GeneratorComponent, ItemComponent,
    MinableProperties, MiningDrillComponent, PlantComponent, ReactorComponent, RecipeComponent,
    ResourceEntityComponent, RocketSiloComponent,
};
use metatorio_data::store::PrototypeGroup;
use metatorio_data::types::{
    BoilerMode, Effect, EffectType, EffectTypeLimitation, EnergyAmount, EnergySource, Ingredient,
    Product,
};

/// Expand all mechanisms in the caller-provided order.
pub fn expand<'a, C: Clone>(
    mechanics: impl IntoIterator<Item = (C, &'a Mechanic)>,
    ctx: &Context,
) -> Expansion<C> {
    let mut expansion = Expansion::default();
    for (config, mechanic) in mechanics {
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
        }
    }
    expansion
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
        .and_then(|record| {
            record.component::<metatorio_data::generated_components::FluidComponent>()
        })
        .map(|fluid| fluid.default_temperature)
        .unwrap_or_default()
}

fn fluid_record<'a>(
    ctx: &'a Context,
    name: &str,
) -> Option<&'a metatorio_data::generated_components::FluidComponent> {
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

fn explicit_fuel<'a>(
    ctx: &Context,
    name: Option<&'a str>,
    temperature: Option<i32>,
    quality: &'a str,
) -> Option<FuelSpec<'a>> {
    let name = name?;
    if ctx.prototype.get(PrototypeGroup::Fluid, name).is_some() {
        Some(FuelSpec::Fluid(FluidFuelSpec {
            fuel: name,
            temperature: temperature
                .map(f64::from)
                .unwrap_or_else(|| fluid_temperature(ctx, name)),
        }))
    } else if ctx.prototype.get(PrototypeGroup::Item, name).is_some() {
        Some(FuelSpec::Item(ItemFuelSpec { name, quality }))
    } else {
        None
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

fn add_machine_effects(
    effects: &mut Effect,
    receiver: Option<&metatorio_data::generated_components::EffectReceiver>,
) {
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
        effect.consumption = 0.0;
    }
    if !allowed(
        EffectType::Speed,
        recipe.is_none_or(|recipe| recipe.allow_speed),
    ) {
        effect.speed = 0.0;
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
        effect.pollution = 0.0;
    }
    if !allowed(
        EffectType::Quality,
        recipe.is_none_or(|recipe| recipe.allow_quality),
    ) {
        effect.quality = 0.0;
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
    let fuel = explicit_fuel(
        ctx,
        mechanic.fuel.as_deref(),
        mechanic.fuel_temperature,
        &mechanic.machine.quality,
    );
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
                let default = fluid_temperature(ctx, &fluid.name);
                let lo = fluid
                    .temperature
                    .or(fluid.minimum_temperature)
                    .unwrap_or(default);
                let hi = fluid
                    .temperature
                    .or(fluid.maximum_temperature)
                    .unwrap_or(default);
                temp.add_fluid(ctx, &fluid.name, -fluid.amount * scale, lo, hi);
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
                temp.add_fluid(
                    ctx,
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
    let fuel = explicit_fuel(
        ctx,
        mechanic.fuel.as_deref(),
        mechanic.fuel_temperature,
        &mechanic.machine.quality,
    );
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
        temp.add_fluid(ctx, fluid, -amount, default, default);
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
                    temp.add_fluid(
                        ctx,
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
    let output_quality = bounded_quality(
        ctx,
        base_quality,
        i32::from(item.spoil_quality_change.unwrap_or_default()),
        item.spoil_quality_min.as_ref(),
        item.spoil_quality_max.as_ref(),
    );
    let mut temp = TempFlow::new();
    temp.add(DualVar::Item(mechanic.item.clone()), -1.0);
    if let Some(result) = &item.spoil_result {
        temp.add(
            DualVar::Item(IdWithQuality::new(
                result,
                quality_name(ctx, output_quality),
            )),
            1.0,
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
                temp.add_fluid(
                    ctx,
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
    temp.add_fluid(
        ctx,
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
    out.variables.extend(temp.into_variables(config));
}

fn add_generator_spent_fluid(
    temp: &mut TempFlow,
    ctx: &Context,
    generator: &GeneratorComponent,
    input: &metatorio_data::generated_components::FluidComponent,
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
    temp.add_fluid(
        ctx,
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
    let fuel = explicit_fuel(
        ctx,
        mechanic.fuel.as_deref(),
        mechanic.fuel_temperature,
        &mechanic.boiler.quality,
    );
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
            temp.add_fluid(
                ctx,
                input_name,
                -output.input_amount_per_second * fulfillment,
                input_temperature,
                input_temperature,
            );
            temp.add_fluid(
                ctx,
                output_name,
                output.output_amount_per_second * fulfillment,
                output.output_temperature,
                output.output_temperature,
            );
        }
    }
    out.variables.extend(temp.into_variables(config));
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
    let fuel = explicit_fuel(
        ctx,
        mechanic.fuel.as_deref(),
        None,
        &mechanic.reactor.quality,
    );
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
    out.variables.extend(temp.into_variables(config));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::GameState;
    use metatorio_data::store::PrototypeStore;
    use serde_json::json;

    #[test]
    fn fluid_temperature_carries_heat() {
        let store = PrototypeStore::load(&json!({
            "fluid": { "water": { "default_temperature": 15.0, "heat_capacity": "1kJ" } }
        }))
        .unwrap();
        let game = GameState::default();
        let ctx = Context::new(&store, &game);
        let mut flow = TempFlow::new();
        flow.add_fluid(&ctx, "water", 2.0, 100.0, 100.0);
        let variables = flow.into_variables("test");
        assert_eq!(variables.len(), 1);
        assert_eq!(
            variables[0].flow.get(&DualVar::FluidHeat {
                filter: "water".to_string(),
            }),
            Some(&170_000.0)
        );
    }
}
