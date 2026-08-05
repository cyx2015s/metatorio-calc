//! 配置 → 虚拟流展开（完整版：recipe + mining）。
//!
//! 把工厂配置展开为原始变量（PrimVar）+ 流系数（迁移自 metatorio-egui
//! RecipeInstance/MiningInstance 的 as_flow）：
//! - 1 个配置可展开为多个代数变量（流体插值：温度区间两端各 1 个，2^k 组合）
//! - **稳定性由调用方保证**：`expand` 按传入顺序分配 config 编号，不做内部排序
//! - 变温流体用 `TempFlow`（内部隐式多副本，添加时自动分裂）
//! - 完整实现：机器能耗（energy_source_as_flow）+ 模块/插件塔效果（get_effect）+
//!   品质分布（calc_quality_distribution）+ 燃料（物品/流体）+ 污染

use crate::NORMAL_QUALITY;
use crate::context::Context;
use crate::dual_var::DualVar;
use crate::energy::{FuelSpec, energy_source_as_flow};
use crate::id::IdWithQuality;
use crate::mechanic::{Mechanic, MiningMechanic, RecipeMechanic, quality_by_level};
use crate::prim_var::Expansion;
use crate::quality::calc_quality_distribution;
use crate::temp_flow::TempFlow;
use metatorio_data::generated_components::{
    CraftingMachineComponent, EntityComponent, MiningDrillComponent, RecipeComponent,
};
use metatorio_data::store::PrototypeGroup;
use metatorio_data::types::{EnergySource, Ingredient, Product};

/// 展开工厂配置为原始变量 + 流系数。
///
/// 配置标识 `C` 由调用方提供（`Hash + Eq`，如配置 ID 或 `AIndexMap` 的键）；
/// 同一配置的多个变量（流体插值端）连续排列，相对位置即端序号。
/// **稳定性由调用方保证**（见模块文档）。
pub fn expand<'a, C: Clone>(
    mechanics: impl Iterator<Item = (C, &'a Mechanic)>,
    ctx: &Context,
) -> Expansion<C> {
    let mut out = Expansion::default();
    for (config, mechanic) in mechanics {
        match mechanic {
            Mechanic::Recipe(m) => expand_recipe(config, m, ctx, &mut out),
            Mechanic::Mining(m) => expand_mining(config, m, ctx, &mut out),
            _ => {}
        }
        // 其余机制后续迭代
    }
    out
}

/// 品质等级 → 品质名（越界 → normal）。
fn quality_name(ctx: &Context, level: usize) -> String {
    ctx.game
        .qualities
        .get(level)
        .cloned()
        .unwrap_or_else(|| NORMAL_QUALITY.to_string())
}

/// 流体的默认温度（原型 FluidComponent.default_temperature，缺省 0）。
fn fluid_default_temperature(ctx: &Context, name: &str) -> f64 {
    ctx.prototype
        .get(PrototypeGroup::Fluid, name)
        .and_then(|r| r.component::<metatorio_data::generated_components::FluidComponent>())
        .map(|f| f.default_temperature)
        .unwrap_or(0.0)
}

/// 配方展开（迁移自 egui RecipeInstance::as_flow）。
///
/// 变量语义：1 秒的配方运行（base_speed = crafting_speed × 品质倍率 ÷ energy_required，
/// 可被 fulfillment 缩放）。
fn expand_recipe<C: Clone>(config: C, m: &RecipeMechanic, ctx: &Context, out: &mut Expansion<C>) {
    let Some(recipe) = ctx
        .prototype
        .get(PrototypeGroup::Recipe, &m.recipe.id)
        .and_then(|r| r.component::<RecipeComponent>())
    else {
        return;
    };
    let quality = ctx.game.quality_level(&m.recipe.quality);

    let mut module_effects = m.module_config.get_effect(ctx);
    if let Some(bonus) = ctx.game.recipe_productivity.get(&m.recipe.id) {
        module_effects.productivity += bonus;
    }

    let mut base_speed = 1.0;
    let mut fulfillment = 1.0;
    let mut temp = TempFlow::new();

    // 机器：基础效果 + 速度 + 能源
    if let Some(crafter) = ctx.prototype.entity(&m.machine.id)
        && let Some(cm) = crafter.component::<CraftingMachineComponent>()
    {
        if let Some(receiver) = &cm.effect_receiver {
            module_effects = module_effects + receiver.base_effect.unwrap_or_default();
        }
        module_effects = module_effects.clamped();
        base_speed = cm.crafting_speed;
        // 品质速度倍率
        let speed_multiplier =
            if let Some(mult) = cm.crafting_speed_quality_multiplier.get(&m.recipe.quality) {
                *mult
            } else {
                quality_by_level(ctx, quality)
                    .map(|q| q.crafting_machine_speed_multiplier())
                    .unwrap_or(1.0)
            };
        base_speed *= speed_multiplier;
        // 能源（燃料温度取流体默认温度；温度插值留待温度敏感机制）
        let fuel = m
            .fuel
            .as_ref()
            .map(|f| FuelSpec::Fluid(f.clone(), fluid_default_temperature(ctx, f)));
        let energy_flow = energy_source_as_flow(
            ctx,
            &cm.energy_source,
            cm.energy_usage,
            &module_effects,
            fuel.as_ref(),
            &mut fulfillment,
        );
        for (key, value) in energy_flow {
            temp.add(key, value);
        }
        // 没有写 drain 的机器：按常态能耗的 1/30 计算
        if let EnergySource::Electric(e) = &cm.energy_source
            && e.drain.is_none()
        {
            temp.add(DualVar::Electricity, -cm.energy_usage.amount * 60.0 / 30.0);
        }
    }

    base_speed /= recipe.energy_required.max(1.0); // 防御：energy_required 缺省/为 0 时按 1 处理
    module_effects.productivity = module_effects
        .productivity
        .clamp(0.0, recipe.maximum_productivity);
    module_effects = module_effects.clamped();

    // 插件塔耗电
    let electric = m.module_config.get_consumption(ctx);
    if electric > 0.0 {
        temp.add(DualVar::Electricity, -electric);
    }

    // 输入（消耗；原料品质 = 配方品质）
    let scale = (1.0 + module_effects.speed) * base_speed * fulfillment;
    for ingredient in &recipe.ingredients {
        match ingredient {
            Ingredient::Item(item) => {
                temp.add(
                    DualVar::Item(IdWithQuality::new(&item.name, m.recipe.quality.clone())),
                    -(item.amount as f64) * scale,
                );
            }
            Ingredient::Fluid(fluid) => {
                let default = fluid_default_temperature(ctx, &fluid.name);
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

    // 品质分布（物品产物）
    let quality_distribution =
        calc_quality_distribution(ctx, module_effects.quality, quality, ctx.game.max_quality);

    // 产物（产出）
    for result in &recipe.results {
        match result {
            Product::Item(item) => {
                let output = item.normalized_output();
                let total_yield = (output.base
                    + output.productivity
                        * module_effects
                            .productivity
                            .clamp(0.0, recipe.maximum_productivity))
                    * scale;
                for (level, &prob) in quality_distribution.iter().enumerate() {
                    if prob > 0.0 {
                        temp.add(
                            DualVar::Item(IdWithQuality::new(&item.name, quality_name(ctx, level))),
                            total_yield * prob,
                        );
                    }
                }
            }
            Product::Fluid(fluid) => {
                let default = fluid
                    .temperature
                    .unwrap_or_else(|| fluid_default_temperature(ctx, &fluid.name));
                let output = fluid.normalized_output();
                temp.add_fluid(
                    ctx,
                    &fluid.name,
                    (output.base
                        + output.productivity
                            * module_effects
                                .productivity
                                .clamp(0.0, recipe.maximum_productivity))
                        * scale,
                    default,
                    default,
                );
            }
        }
    }

    out.variables.extend(temp.into_variables(config));
}

/// 采矿展开（迁移自 egui MiningInstance::as_flow）。
///
/// 变量语义：1 秒的采矿运行（base_speed = mining_speed × 品质倍率 ÷ mining_time）。
fn expand_mining<C: Clone>(config: C, m: &MiningMechanic, ctx: &Context, out: &mut Expansion<C>) {
    let quality = ctx.game.quality_level(&m.machine.quality);
    let mut module_effects = m.module_config.get_effect(ctx);
    let mut base_speed = 1.0;
    let mut drain_rate = quality_by_level(ctx, quality)
        .map(|q| q.mining_drill_resource_drain_multiplier)
        .unwrap_or(1.0);
    let mut fulfillment = 1.0;
    let mut temp = TempFlow::new();

    // 采矿机：基础效果 + 速度 + 能源
    if let Some(miner) = ctx.prototype.entity(&m.machine.id)
        && let Some(md) = miner.component::<MiningDrillComponent>()
    {
        if let Some(receiver) = &md.effect_receiver {
            module_effects = module_effects + receiver.base_effect.unwrap_or_default();
        }
        module_effects.productivity += if md.uses_force_mining_productivity_bonus {
            ctx.game.mining_productivity
        } else {
            0.0
        };
        module_effects = module_effects.clamped();
        base_speed = md.mining_speed;
        drain_rate *= md.resource_drain_rate_percent.unwrap_or(100) as f64 / 100.0;
        // 能源（物品燃料）
        let fuel = m
            .fuel
            .as_ref()
            .map(|f| FuelSpec::Fluid(f.clone(), fluid_default_temperature(ctx, f)));
        let energy_flow = energy_source_as_flow(
            ctx,
            &md.energy_source,
            md.energy_usage,
            &module_effects,
            fuel.as_ref(),
            &mut fulfillment,
        );
        for (key, value) in energy_flow {
            temp.add(key, value);
        }
        if let EnergySource::Electric(e) = &md.energy_source
            && e.drain.is_none()
        {
            temp.add(DualVar::Electricity, -md.energy_usage.amount * 60.0 / 30.0);
        }
    }

    // 矿脉
    let Some(resource) = ctx.prototype.entity(&m.resource) else {
        return;
    };
    let Some(entity) = resource.component::<EntityComponent>() else {
        return;
    };
    let Some(minable) = entity.minable() else {
        return;
    };
    base_speed /= minable.mining_time;

    // 矿脉实体本身的消耗
    temp.add(
        DualVar::Entity(IdWithQuality::new(&m.resource, NORMAL_QUALITY)),
        -base_speed * (1.0 + module_effects.speed) * drain_rate * fulfillment,
    );

    // 插件塔耗电
    let electric = m.module_config.get_consumption(ctx);
    if electric > 0.0 {
        temp.add(DualVar::Electricity, -electric);
    }

    // 开采流体消耗（默认温度单点；矿脉要求的流体温度固定）
    if let Some(fluid) = &minable.required_fluid {
        let default = fluid_default_temperature(ctx, fluid);
        let amount =
            base_speed * (1.0 + module_effects.speed) * minable.fluid_amount / 10.0 * fulfillment;
        temp.add_fluid(ctx, fluid, -amount, default, default);
    }

    // 品质分布（产物品质从 normal 起）
    let quality_distribution =
        calc_quality_distribution(ctx, module_effects.quality, 0, ctx.game.max_quality);
    let scale = (1.0 + module_effects.speed) * base_speed * fulfillment;

    if let Some(result) = &minable.result {
        // 单结果：count 产物
        let count = minable.count.unwrap_or(1) as f64;
        let total_yield = scale * count * (1.0 + module_effects.productivity);
        for (level, &prob) in quality_distribution.iter().enumerate() {
            if prob > 0.0 {
                temp.add(
                    DualVar::Item(IdWithQuality::new(result, quality_name(ctx, level))),
                    total_yield * prob,
                );
            }
        }
    } else {
        // 多结果
        for result in &minable.results {
            match result {
                Product::Item(r) => {
                    let output = r.normalized_output();
                    let total_yield =
                        scale * (output.base + output.productivity * module_effects.productivity);
                    for (level, &prob) in quality_distribution.iter().enumerate() {
                        if prob > 0.0 {
                            temp.add(
                                DualVar::Item(IdWithQuality::new(
                                    &r.name,
                                    quality_name(ctx, level),
                                )),
                                total_yield * prob,
                            );
                        }
                    }
                }
                Product::Fluid(r) => {
                    let output = r.normalized_output();
                    let default = r
                        .temperature
                        .unwrap_or_else(|| fluid_default_temperature(ctx, &r.name));
                    temp.add_fluid(
                        ctx,
                        &r.name,
                        scale * (output.base + output.productivity * module_effects.productivity),
                        default,
                        default,
                    );
                }
            }
        }
    }

    out.variables.extend(temp.into_variables(config));
}
