//! 延迟列生成（Delayed Column Generation）自动规划引擎。
//!
//! 将原本一次性全量枚举全部 `配方 × 机器 × 插件组合 × 插件塔 × 品质` 的做法改为：
//! 1. **极小种子集**：每个 `(配方/采矿, 机器, 品质, 插件塔)` 仅向初始受限主问题（RMP）放入
//!    「空槽 + 纯单一插件」列；
//! 2. **截断后效应去重**：同一分组下，经 `restrict_effect` / `maximum_productivity` / `clamped`
//!    后产生完全相同有效效应的插件组合只保留一个代表；
//! 3. **Phase I（Farkas 定价）+ Phase II（对偶定价）**：
//!    - 若初始种子集不可行（例如高倍率品质模组跳过相邻品质，必须混插速度插件降品），
//!      自动切入 Phase I 取 Farkas 影子价格拉入可行混合列；
//!    - Phase II 从 HiGHS 提取精确行对偶并做纯标量检验数（Reduced Cost）定价，
//!      只把检验数 `< -tol` 的组合实例化并通过 `HighsColumnGenMaster::add_flow` 热启动追加。

use std::collections::HashSet;
use std::time::Instant;

use metatorio_core::expand::instance_cost;
use metatorio_core::mechanic::{BeaconConfig, module_effect_at_quality};
use metatorio_core::quality::calc_quality_distribution;
use metatorio_core::{
    Accessibility, Context, DualVar, IdWithQuality, Mechanic, MiningMechanic, ModuleConfig,
    RecipeMechanic,
};
use metatorio_data::store::{PrototypeGroup, PrototypeStore};
use metatorio_data::types::{
    Effect, EffectType, EffectTypeLimitation, EnergySource, Ingredient, Product,
};
use metatorio_data::{
    CraftingMachineComponent, EntityComponent, FluidComponent, MiningDrillComponent,
    ModuleComponent, RecipeComponent, ResourceEntityComponent, RocketSiloComponent,
};
use metatorio_solver::column_gen::{HighsColumnGenMaster, MasterSolveState};
use metatorio_solver::{AIndexMap, LpSolver, SolverData};

use crate::auto_plan::{
    EnumerateOptions, SurfaceEnvironment, buildable_beacon_plans, buildable_on_surface,
    crafter_score, entity_available, enumerate_energy, enumerate_simple, enumerate_tiles,
    machine_fits_recipe, miner_score, module_allowed, module_combinations, pick_machines,
    quality_name, recipe_unlocked,
};
use crate::id::MechanicId;
use crate::solve::ExpandedVarId;

/// 列生成求解过程的统计与性能指标。
#[derive(Debug, Clone, Default)]
pub struct ColumnGenStats {
    /// 初始放进 RMP 的种子候选数。
    pub seed_candidates: usize,
    /// 若全量枚举（去重后）对应的总候选数。
    pub total_unique_candidates: usize,
    /// 原始全量枚举（去重前）的总候选数。
    pub raw_full_candidates: usize,
    /// 列生成过程中按需追加的新列数。
    pub added_columns: usize,
    /// Phase I 迭代轮数（初始种子可行时为 0）。
    pub phase1_rounds: usize,
    /// Phase II 迭代轮数。
    pub phase2_rounds: usize,
    /// 最终 RMP 的列数（含源/汇/转换流）。
    pub final_cols: usize,
    /// 最终 RMP 的约束行数。
    pub final_rows: usize,
    /// 种子与定价分组构造耗时（毫秒）。
    pub enumerate_ms: f64,
    /// HiGHS RMP 求解总耗时（毫秒）。
    pub master_solve_ms: f64,
    /// 定价（Pricing）扫描总耗时（毫秒）。
    pub pricing_ms: f64,
}

#[derive(Debug, Clone)]
struct ComboCandidate {
    counts: Vec<usize>,
    dup: usize,
    effective: Effect,
    added: bool,
}

#[derive(Debug, Clone)]
struct RecipeItemOut {
    name: String,
    base: f64,
    prod: f64,
    affected_by_quality: bool,
    quality_change: i32,
    quality_min: Option<String>,
    quality_max: Option<String>,
}

/// 电驱动配方的纯标量定价缓存（与 `expand_recipe` 逐项等价）。
#[derive(Debug, Clone)]
struct FastElectricRecipe {
    fixed_cost: f64,
    heating_watts: f64,
    beacon_watts: f64,
    /// `usage_base + drain_base`，实际机器电耗为 `base_energy_watts * (1 + effects.consumption)`。
    base_energy_watts: f64,
    base_rate: f64,
    ingredients: Vec<(DualVar, f64)>,
    item_outputs: Vec<RecipeItemOut>,
    fluid_outputs: Vec<(DualVar, f64, f64)>,
    rocket_silo_output: Option<(DualVar, f64)>,
}

#[derive(Debug, Clone)]
struct RecipePricingGroup {
    recipe_id: String,
    recipe_quality: usize,
    recipe_quality_name: String,
    machine: IdWithQuality,
    beacons: Vec<BeaconConfig>,
    allowed_modules: Vec<IdWithQuality>,
    combos: Vec<ComboCandidate>,
    fast_electric: Option<FastElectricRecipe>,
}

#[derive(Debug, Clone)]
struct MiningPricingGroup {
    resource: String,
    machine: IdWithQuality,
    beacons: Vec<BeaconConfig>,
    allowed_modules: Vec<IdWithQuality>,
    combos: Vec<ComboCandidate>,
}

#[derive(Debug, Clone)]
enum PricingGroup {
    Recipe(RecipePricingGroup),
    Mining(MiningPricingGroup),
}

impl PricingGroup {
    fn materialize(&self, combo_idx: usize) -> Mechanic {
        match self {
            PricingGroup::Recipe(g) => {
                let combo = &g.combos[combo_idx];
                let modules = build_module_vec(&g.allowed_modules, &combo.counts, combo.dup);
                Mechanic::Recipe(RecipeMechanic {
                    recipe: IdWithQuality::new(g.recipe_id.clone(), g.recipe_quality_name.clone()),
                    machine: g.machine.clone(),
                    module_config: ModuleConfig {
                        modules,
                        beacons: g.beacons.clone(),
                    },
                    fuel: None,
                })
            }
            PricingGroup::Mining(g) => {
                let combo = &g.combos[combo_idx];
                let modules = build_module_vec(&g.allowed_modules, &combo.counts, combo.dup);
                Mechanic::Mining(MiningMechanic {
                    resource: g.resource.clone(),
                    machine: g.machine.clone(),
                    module_config: ModuleConfig {
                        modules,
                        beacons: g.beacons.clone(),
                    },
                    fuel: None,
                })
            }
        }
    }
}

fn build_module_vec(
    allowed_modules: &[IdWithQuality],
    counts: &[usize],
    dup: usize,
) -> Vec<IdWithQuality> {
    let mut modules = Vec::new();
    for (idx, module) in allowed_modules.iter().enumerate() {
        let count = counts.get(idx).copied().unwrap_or(0) * dup;
        for _ in 0..count {
            modules.push(module.clone());
        }
    }
    modules
}

fn norm_f64_bits(v: f64) -> u64 {
    if v == 0.0 { 0.0f64.to_bits() } else { v.to_bits() }
}

fn effect_key(e: &Effect) -> (u64, u64, u64, u64, u64) {
    (
        norm_f64_bits(e.speed),
        norm_f64_bits(e.productivity),
        norm_f64_bits(e.consumption),
        norm_f64_bits(e.quality),
        norm_f64_bits(e.pollution),
    )
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
        recipe.is_none_or(|r| r.allow_consumption),
    ) {
        effect.consumption = effect.consumption.max(0.0);
    }
    if !allowed(EffectType::Speed, recipe.is_none_or(|r| r.allow_speed)) {
        effect.speed = effect.speed.min(0.0);
    }
    if !allowed(
        EffectType::Productivity,
        recipe.is_none_or(|r| r.allow_productivity),
    ) {
        effect.productivity = 0.0;
    }
    if !allowed(
        EffectType::Pollution,
        recipe.is_none_or(|r| r.allow_pollution),
    ) {
        effect.pollution = effect.pollution.max(0.0);
    }
    if !allowed(EffectType::Quality, recipe.is_none_or(|r| r.allow_quality)) {
        effect.quality = effect.quality.min(0.0);
    }
    effect
}

fn compute_effective(
    module_effects: &[Effect],
    counts: &[usize],
    dup: usize,
    beacon_effect: Effect,
    machine_allowed: Option<&EffectTypeLimitation>,
    recipe: Option<&RecipeComponent>,
    extra_productivity: f64,
    extra_pollution: f64,
    base_effect: Effect,
    max_productivity: Option<f64>,
) -> Effect {
    let mut raw = beacon_effect;
    for (idx, eff) in module_effects.iter().enumerate() {
        let n = counts.get(idx).copied().unwrap_or(0) * dup;
        if n > 0 {
            let nf = n as f64;
            raw.consumption += eff.consumption * nf;
            raw.speed += eff.speed * nf;
            raw.productivity += eff.productivity * nf;
            raw.pollution += eff.pollution * nf;
            raw.quality += eff.quality * nf;
        }
    }
    let mut eff = restrict_effect(raw, machine_allowed, recipe);
    eff.productivity += extra_productivity;
    eff.pollution += extra_pollution;
    eff = (eff + base_effect).clamped();
    if let Some(max_p) = max_productivity {
        eff.productivity = eff.productivity.clamp(0.0, max_p);
        eff = eff.clamped();
    }
    eff
}

fn quality_level(ctx: &Context, name: &str) -> usize {
    ctx.game
        .qualities
        .iter()
        .position(|q| q == name)
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
        .and_then(|record| record.component::<FluidComponent>())
        .map(|fluid| fluid.default_temperature)
        .unwrap_or_default()
}

fn fluid_max_temperature(ctx: &Context, name: &str) -> f64 {
    ctx.prototype
        .get(PrototypeGroup::Fluid, name)
        .and_then(|record| record.component::<FluidComponent>())
        .map(|fluid| fluid.max_temperature())
        .unwrap_or_default()
}

fn precompute_module_effects(
    store: &PrototypeStore,
    ctx: &Context,
    allowed_modules: &[IdWithQuality],
) -> Vec<Effect> {
    let qualities: Vec<Option<&metatorio_data::QualityComponent>> = ctx
        .prototype
        .quality_order()
        .iter()
        .map(|name| {
            store
                .get(PrototypeGroup::Quality, name)
                .and_then(|r| r.component::<metatorio_data::QualityComponent>())
        })
        .collect();
    allowed_modules
        .iter()
        .map(|m| {
            let Some(proto) = store
                .get(PrototypeGroup::Item, &m.id)
                .and_then(|r| r.component::<ModuleComponent>())
            else {
                return Effect::default();
            };
            let ql = ctx.game.quality_level(&m.quality);
            qualities
                .get(ql)
                .and_then(|q| *q)
                .map(|q| module_effect_at_quality(proto, q))
                .unwrap_or(proto.effect)
        })
        .collect()
}

/// 构造电驱动配方的快速标量定价缓存（与 `expand_recipe` 精确一致）。
fn build_fast_electric_recipe(
    store: &PrototypeStore,
    ctx: &Context,
    recipe_id: &str,
    recipe_quality: usize,
    recipe: &RecipeComponent,
    machine_id: &IdWithQuality,
    crafter_record: &metatorio_data::store::PrototypeRecord,
    machine: &CraftingMachineComponent,
    beacons: &[BeaconConfig],
) -> Option<FastElectricRecipe> {
    let EnergySource::Electric(electric) = &machine.energy_source else {
        return None;
    };
    if !machine.supports_recipe_fluid_input(recipe) {
        return None;
    }
    let recipe_time = if recipe.energy_required > 0.0 {
        recipe.energy_required
    } else {
        0.5
    };
    let q_comp = ctx
        .prototype
        .quality_order()
        .get(quality_level(ctx, &machine_id.quality))
        .and_then(|name| store.get(PrototypeGroup::Quality, name))
        .and_then(|r| r.component::<metatorio_data::QualityComponent>());

    let speed_multiplier = machine
        .crafting_speed_quality_multiplier
        .get(&machine_id.quality)
        .copied()
        .or_else(|| q_comp.map(|q| q.crafting_machine_speed_multiplier()))
        .unwrap_or(1.0);
    let energy_multiplier = if machine.quality_affects_energy_usage {
        q_comp
            .map(|q| q.crafting_machine_energy_usage_multiplier)
            .unwrap_or(1.0)
    } else {
        1.0
    };

    let base_rate = machine.crafting_speed * speed_multiplier / recipe_time;
    let usage_base = machine.energy_usage.amount * energy_multiplier * 60.0;
    let drain_base = if let Some(drain) = electric.drain {
        drain.amount * 60.0
    } else {
        usage_base / 30.0
    };
    let base_energy_watts = usage_base + drain_base;

    let dummy_config = ModuleConfig {
        modules: Vec::new(),
        beacons: beacons.to_vec(),
    };
    let beacon_watts = dummy_config.get_consumption_for(ctx, machine.effect_receiver.as_ref());

    let dummy_mech = Mechanic::Recipe(RecipeMechanic {
        recipe: IdWithQuality::new(recipe_id, quality_name(ctx, recipe_quality)),
        machine: machine_id.clone(),
        module_config: dummy_config,
        fuel: None,
    });
    let fixed_cost = instance_cost(store, &dummy_mech);

    let heating_watts = if ctx.game.entities_require_heating {
        let entity_heat = |name: &str| {
            store
                .entity(name)
                .and_then(|r| r.component::<EntityComponent>())
                .map_or(0.0, |e| e.heating_energy().amount * 60.0)
        };
        let b_heat: f64 = beacons
            .iter()
            .map(|b| entity_heat(&b.beacon.id) * b.count as f64 / b.share.max(1.0))
            .sum();
        entity_heat(&machine_id.id) + b_heat
    } else {
        0.0
    };

    let mut ingredients = Vec::with_capacity(recipe.ingredients.len());
    for ingredient in &recipe.ingredients {
        match ingredient {
            Ingredient::Item(item) => {
                let iq = bounded_quality(
                    ctx,
                    recipe_quality,
                    i32::from(item.quality_change),
                    item.quality_min.as_ref(),
                    item.quality_max.as_ref(),
                );
                ingredients.push((
                    DualVar::Item(IdWithQuality::new(&item.name, quality_name(ctx, iq))),
                    f64::from(item.amount),
                ));
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
                ingredients.push((
                    DualVar::Fluid {
                        name: fluid.name.clone(),
                        temperature: [lo as i32, hi as i32],
                    },
                    fluid.amount,
                ));
            }
        }
    }

    let mut item_outputs = Vec::new();
    let mut fluid_outputs = Vec::new();
    let mut rocket_silo_output = None;

    if let Some(silo) = crafter_record.component::<RocketSiloComponent>() {
        let parts = silo.rocket_parts_required.max(1) as f64;
        if silo.launch_to_space_platforms {
            let lift_weight = if silo.lift_weight > 0.0 {
                silo.lift_weight
            } else {
                store.weight_constants().default_rocket_lift_weight
            };
            rocket_silo_output = Some((DualVar::RocketWeightCapacity, lift_weight / parts));
        } else {
            let slots = silo.to_be_inserted_to_rocket_inventory_size.unwrap_or_default() as f64;
            rocket_silo_output = Some((DualVar::RocketSlotCapacity, slots / parts));
        }
    } else {
        for result in &recipe.results {
            match result {
                Product::Item(item) => {
                    let output = item.normalized_output();
                    item_outputs.push(RecipeItemOut {
                        name: item.name.clone(),
                        base: output.base,
                        prod: output.productivity,
                        affected_by_quality: item.affected_by_quality,
                        quality_change: i32::from(item.quality_change),
                        quality_min: item.quality_min.clone(),
                        quality_max: item.quality_max.clone(),
                    });
                }
                Product::Fluid(fluid) => {
                    let temperature = fluid
                        .temperature
                        .unwrap_or_else(|| fluid_temperature(ctx, &fluid.name));
                    let output = fluid.normalized_output();
                    let t = temperature as i32;
                    fluid_outputs.push((
                        DualVar::Fluid {
                            name: fluid.name.clone(),
                            temperature: [t, t],
                        },
                        output.base,
                        output.productivity,
                    ));
                }
            }
        }
    }

    Some(FastElectricRecipe {
        fixed_cost,
        heating_watts,
        beacon_watts,
        base_energy_watts,
        base_rate,
        ingredients,
        item_outputs,
        fluid_outputs,
        rocket_silo_output,
    })
}

/// 枚举列生成所需的「初始种子机制」与「延迟定价分组」。
fn enumerate_seed_and_groups(
    store: &PrototypeStore,
    ctx: &Context,
    options: &EnumerateOptions,
    accessibility: &Accessibility,
) -> (Vec<Mechanic>, Vec<PricingGroup>, usize, usize) {
    let environment = SurfaceEnvironment::new(store, options);
    let mut seeds = Vec::new();
    let mut groups = Vec::new();
    let mut raw_full_candidates = 0usize;
    let mut total_unique_candidates = 0usize;

    // 1) 非插件机制直接进入种子集
    enumerate_simple(store, ctx, options, &environment, &mut seeds);
    enumerate_energy(store, ctx, options, &environment, &mut seeds);
    enumerate_tiles(store, ctx, options, &environment, &mut seeds);
    raw_full_candidates += seeds.len();
    total_unique_candidates += seeds.len();

    let quality_range = options.quality_limit + 1;
    let major_quality = quality_name(ctx, options.major_quality);
    let beacons = buildable_beacon_plans(store, options, &environment);

    // 构造插件塔列表：[无插件塔, ...已启用的插件塔方案]
    let mut beacon_variants: Vec<Vec<BeaconConfig>> = Vec::with_capacity(beacons.len() + 1);
    beacon_variants.push(Vec::new());
    for plan in &beacons {
        beacon_variants.push(plan.module_config.beacons.clone());
    }

    // 2) 配方：前置过滤不可达配方，每个 (recipe, machine, quality, beacon) 仅放种子列，其余进定价组
    for record in store.group(PrototypeGroup::Recipe) {
        let Some(recipe) = record.component::<RecipeComponent>() else {
            continue;
        };
        if !recipe_unlocked(store, accessibility, &record.name) {
            continue;
        }
        if let Some(properties) = &environment.properties
            && !crate::planet::surface_condition_satisfied(
                store,
                &recipe.surface_conditions,
                properties,
            )
        {
            continue;
        }
        let has_item_ingredient = recipe
            .ingredients
            .iter()
            .any(|ingredient| matches!(ingredient, Ingredient::Item(_)));
        let recipe_quality_range = if has_item_ingredient { quality_range } else { 1 };
        let machines = pick_machines(
            store,
            &options.machine_preferences,
            options.alternative_count,
            &major_quality,
            |r| {
                r.component::<CraftingMachineComponent>()
                    .is_some_and(|m| machine_fits_recipe(m, recipe))
                    && entity_available(store, r, options, &environment)
            },
            |r| {
                r.component::<CraftingMachineComponent>()
                    .zip(r.component::<EntityComponent>())
                    .map(|(m, e)| crafter_score(m, Some(e)))
                    .unwrap_or(0.0)
            },
        );
        let tech_prod = ctx
            .game
            .recipe_productivity
            .get(&record.name)
            .copied()
            .unwrap_or_default();
        let extra_pol = recipe.emissions_multiplier - 1.0;

        for machine in &machines {
            let Some(machine_record) = store.get(PrototypeGroup::Entity, &machine.id) else {
                continue;
            };
            let Some(machine_component) = machine_record.component::<CraftingMachineComponent>()
            else {
                continue;
            };
            let receiver = machine_component.effect_receiver.as_ref();
            let uses_modules = receiver.is_none_or(|r| r.uses_module_effects);
            let uses_beacons = receiver.is_none_or(|r| r.uses_beacon_effects);
            let base_effect = receiver.and_then(|r| r.base_effect).unwrap_or_default();

            let allowed_modules: Vec<IdWithQuality> = if uses_modules {
                options
                    .enumerate_modules
                    .iter()
                    .filter(|module_name| {
                        store
                            .get(PrototypeGroup::Item, &module_name.id)
                            .and_then(|r| r.component::<ModuleComponent>())
                            .is_some_and(|module| {
                                module_allowed(
                                    module,
                                    &machine_component.allowed_module_categories,
                                    &machine_component.allowed_effects,
                                    Some(recipe),
                                )
                            })
                    })
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            };
            let quality_involved = allowed_modules.iter().any(|m| {
                store
                    .get(PrototypeGroup::Item, &m.id)
                    .and_then(|r| r.component::<ModuleComponent>())
                    .is_some_and(|module| module.effect.quality > 0.0)
            });
            let module_slots = machine_component.module_slots.map(|s| s as usize).unwrap_or(0);
            let raw_combos =
                module_combinations(allowed_modules.len(), module_slots, quality_involved);
            let mod_effects = precompute_module_effects(store, ctx, &allowed_modules);

            let active_beacons = if uses_beacons {
                &beacon_variants[..]
            } else {
                &beacon_variants[..1]
            };

            for beacon_cfg in active_beacons {
                let beacon_only_cfg = ModuleConfig {
                    modules: Vec::new(),
                    beacons: beacon_cfg.clone(),
                };
                let beacon_effect = beacon_only_cfg.get_effect_for(ctx, receiver);

                // 按截断后的有效效应去重，并选出种子（空槽 + 纯单一插件）
                let mut seen: HashSet<(u64, u64, u64, u64, u64)> = HashSet::new();
                let mut deduped: Vec<ComboCandidate> = Vec::new();

                // 先放种子：空槽（全 0）+ 各纯单一插件填满槽位
                let mut seed_specs: Vec<(Vec<usize>, usize)> = Vec::new();
                seed_specs.push((vec![0usize; allowed_modules.len()], 1));
                if module_slots > 0 {
                    let pure_slots = if allowed_modules.len() > 5 {
                        module_slots.min(24)
                    } else if allowed_modules.len() > 2 && module_slots > 16 {
                        16
                    } else if allowed_modules.len() > 1 && module_slots > 24 {
                        24
                    } else {
                        module_slots
                    };
                    for m_idx in 0..allowed_modules.len() {
                        let mut c = vec![0usize; allowed_modules.len()];
                        c[m_idx] = pure_slots;
                        seed_specs.push((c, 1));
                    }
                }

                for (counts, dup) in seed_specs {
                    let eff = compute_effective(
                        &mod_effects,
                        &counts,
                        dup,
                        beacon_effect,
                        machine_component.allowed_effects.as_ref(),
                        Some(recipe),
                        tech_prod,
                        extra_pol,
                        base_effect,
                        Some(recipe.maximum_productivity),
                    );
                    if seen.insert(effect_key(&eff)) {
                        deduped.push(ComboCandidate {
                            counts,
                            dup,
                            effective: eff,
                            added: true,
                        });
                    }
                }

                let seed_count = deduped.len();

                // 再放其余混合组合（延迟定价）
                for (counts, dup) in &raw_combos {
                    let eff = compute_effective(
                        &mod_effects,
                        counts,
                        *dup,
                        beacon_effect,
                        machine_component.allowed_effects.as_ref(),
                        Some(recipe),
                        tech_prod,
                        extra_pol,
                        base_effect,
                        Some(recipe.maximum_productivity),
                    );
                    if seen.insert(effect_key(&eff)) {
                        deduped.push(ComboCandidate {
                            counts: counts.clone(),
                            dup: *dup,
                            effective: eff,
                            added: false,
                        });
                    }
                }

                for q in 0..recipe_quality_range {
                    raw_full_candidates += raw_combos.len();
                    total_unique_candidates += deduped.len();
                    let q_name = quality_name(ctx, q);

                    // 写入种子列
                    for combo in &deduped[..seed_count] {
                        let modules = build_module_vec(&allowed_modules, &combo.counts, combo.dup);
                        seeds.push(Mechanic::Recipe(RecipeMechanic {
                            recipe: IdWithQuality::new(record.name.clone(), q_name.clone()),
                            machine: machine.clone(),
                            module_config: ModuleConfig {
                                modules,
                                beacons: beacon_cfg.clone(),
                            },
                            fuel: None,
                        }));
                    }

                    // 只有存在未加入的混合组合时，才需要建立 PricingGroup
                    if deduped.len() > seed_count {
                        let fast_electric = build_fast_electric_recipe(
                            store,
                            ctx,
                            &record.name,
                            q,
                            recipe,
                            machine,
                            machine_record,
                            machine_component,
                            beacon_cfg,
                        );
                        groups.push(PricingGroup::Recipe(RecipePricingGroup {
                            recipe_id: record.name.clone(),
                            recipe_quality: q,
                            recipe_quality_name: q_name,
                            machine: machine.clone(),
                            beacons: beacon_cfg.clone(),
                            allowed_modules: allowed_modules.clone(),
                            combos: deduped.clone(),
                            fast_electric,
                        }));
                    }
                }
            }
        }
    }

    // 3) 采矿：同样只放种子列，混合组合进定价组
    if !crate::planet::is_space_platform(options.surface.as_deref()) {
        for record in store.group(PrototypeGroup::Entity) {
            if record.type_ != "resource" {
                continue;
            }
            let Some(resource) = record.component::<ResourceEntityComponent>() else {
                continue;
            };
            if !buildable_on_surface(store, record, &environment) {
                continue;
            }
            let category = if resource.category.is_empty() {
                "basic-solid".to_string()
            } else {
                resource.category.clone()
            };
            let machines = pick_machines(
                store,
                &options.machine_preferences,
                options.alternative_count,
                &major_quality,
                |drill_record| {
                    drill_record
                        .component::<MiningDrillComponent>()
                        .is_some_and(|drill| drill.resource_categories.contains(&category))
                        && entity_available(store, drill_record, options, &environment)
                },
                |drill_record| {
                    drill_record
                        .component::<MiningDrillComponent>()
                        .zip(drill_record.component::<EntityComponent>())
                        .map(|(miner, entity)| miner_score(miner, Some(entity)))
                        .unwrap_or(0.0)
                },
            );
            for machine in machines {
                let Some(drill_record) = store.get(PrototypeGroup::Entity, &machine.id) else {
                    continue;
                };
                let Some(drill) = drill_record.component::<MiningDrillComponent>() else {
                    continue;
                };
                let receiver = drill.effect_receiver.as_ref();
                let uses_modules = receiver.is_none_or(|r| r.uses_module_effects);
                let uses_beacons = receiver.is_none_or(|r| r.uses_beacon_effects);
                let base_effect = receiver.and_then(|r| r.base_effect).unwrap_or_default();
                let extra_prod = if drill.uses_force_mining_productivity_bonus {
                    ctx.game.mining_productivity
                } else {
                    0.0
                };

                let allowed_modules: Vec<IdWithQuality> = if uses_modules {
                    options
                        .enumerate_modules
                        .iter()
                        .filter(|module_name| {
                            store
                                .get(PrototypeGroup::Item, &module_name.id)
                                .and_then(|r| r.component::<ModuleComponent>())
                                .is_some_and(|module| {
                                    module_allowed(
                                        module,
                                        &drill.allowed_module_categories,
                                        &drill.allowed_effects,
                                        None,
                                    )
                                })
                        })
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                };
                let quality_involved = allowed_modules.iter().any(|m| {
                    store
                        .get(PrototypeGroup::Item, &m.id)
                        .and_then(|r| r.component::<ModuleComponent>())
                        .is_some_and(|module| module.effect.quality > 0.0)
                });
                let module_slots = drill.module_slots.map(|s| s as usize).unwrap_or(0);
                let raw_combos =
                    module_combinations(allowed_modules.len(), module_slots, quality_involved);
                let mod_effects = precompute_module_effects(store, ctx, &allowed_modules);

                let active_beacons = if uses_beacons {
                    &beacon_variants[..]
                } else {
                    &beacon_variants[..1]
                };

                for beacon_cfg in active_beacons {
                    let beacon_effect = ModuleConfig {
                        modules: Vec::new(),
                        beacons: beacon_cfg.clone(),
                    }
                    .get_effect_for(ctx, receiver);

                    let mut seen: HashSet<(u64, u64, u64, u64, u64)> = HashSet::new();
                    let mut deduped: Vec<ComboCandidate> = Vec::new();

                    let mut seed_specs: Vec<(Vec<usize>, usize)> = Vec::new();
                    seed_specs.push((vec![0usize; allowed_modules.len()], 1));
                    if module_slots > 0 {
                        let pure_slots = module_slots.min(24);
                        for m_idx in 0..allowed_modules.len() {
                            let mut c = vec![0usize; allowed_modules.len()];
                            c[m_idx] = pure_slots;
                            seed_specs.push((c, 1));
                        }
                    }

                    for (counts, dup) in seed_specs {
                        let eff = compute_effective(
                            &mod_effects,
                            &counts,
                            dup,
                            beacon_effect,
                            drill.allowed_effects.as_ref(),
                            None,
                            extra_prod,
                            0.0,
                            base_effect,
                            None,
                        );
                        if seen.insert(effect_key(&eff)) {
                            deduped.push(ComboCandidate {
                                counts,
                                dup,
                                effective: eff,
                                added: true,
                            });
                        }
                    }

                    let seed_count = deduped.len();
                    for (counts, dup) in &raw_combos {
                        let eff = compute_effective(
                            &mod_effects,
                            counts,
                            *dup,
                            beacon_effect,
                            drill.allowed_effects.as_ref(),
                            None,
                            extra_prod,
                            0.0,
                            base_effect,
                            None,
                        );
                        if seen.insert(effect_key(&eff)) {
                            deduped.push(ComboCandidate {
                                counts: counts.clone(),
                                dup: *dup,
                                effective: eff,
                                added: false,
                            });
                        }
                    }

                    raw_full_candidates += raw_combos.len();
                    total_unique_candidates += deduped.len();

                    for combo in &deduped[..seed_count] {
                        let modules = build_module_vec(&allowed_modules, &combo.counts, combo.dup);
                        seeds.push(Mechanic::Mining(MiningMechanic {
                            resource: record.name.clone(),
                            machine: machine.clone(),
                            module_config: ModuleConfig {
                                modules,
                                beacons: beacon_cfg.clone(),
                            },
                            fuel: None,
                        }));
                    }

                    if deduped.len() > seed_count {
                        groups.push(PricingGroup::Mining(MiningPricingGroup {
                            resource: record.name.clone(),
                            machine: machine.clone(),
                            beacons: beacon_cfg.clone(),
                            allowed_modules: allowed_modules.clone(),
                            combos: deduped,
                        }));
                    }
                }
            }
        }
    }

    (seeds, groups, raw_full_candidates, total_unique_candidates)
}

/// 对单个配方定价组执行纯标量检验数扫描，返回最负检验数的 `(combo_idx, reduced_cost)`。
fn price_recipe_group(
    group: &RecipePricingGroup,
    ctx: &Context,
    duals: &AIndexMap<DualVar, f64>,
    phase1_zero_cost: bool,
    tol: f64,
) -> Option<(usize, f64)> {
    let Some(fast) = &group.fast_electric else {
        // 极少数非电驱动配方回退到精确 expand 计算内积
        return price_via_expand(
            &PricingGroup::Recipe(group.clone()),
            &group.combos,
            ctx,
            duals,
            phase1_zero_cost,
            tol,
        );
    };

    let pi_elec = duals.get(&DualVar::Electricity).copied().unwrap_or(0.0);
    let pi_heat = duals.get(&DualVar::Heat).copied().unwrap_or(0.0);

    let c0 = if phase1_zero_cost {
        0.0
    } else {
        fast.fixed_cost
    } + pi_heat * fast.heating_watts
        + pi_elec * fast.beacon_watts;

    let mut v_in = 0.0;
    for (var, amt) in &fast.ingredients {
        if let Some(&pi) = duals.get(var) {
            v_in += pi * amt;
        }
    }

    let mut v_const_base = 0.0;
    let mut v_const_prod = 0.0;
    for (var, base, prod) in &fast.fluid_outputs {
        if let Some(&pi) = duals.get(var) {
            v_const_base += pi * base;
            v_const_prod += pi * prod;
        }
    }
    if let Some((var, per_craft)) = &fast.rocket_silo_output
        && let Some(&pi) = duals.get(var)
    {
        v_const_base += pi * per_craft;
        v_const_prod += pi * per_craft;
    }

    let q_count = quality_count(ctx);
    let q_limit = quality_limit(ctx);

    // 预提取每项物品产物在各品质上的影子价格，避免内层循环里做 HashMap 查找与 String 分配
    let item_dual_tables: Vec<(f64, f64, bool, Vec<f64>)> = fast
        .item_outputs
        .iter()
        .map(|out| {
            let mut table = vec![0.0; q_count];
            if !out.affected_by_quality {
                let oq = bounded_quality(
                    ctx,
                    group.recipe_quality,
                    out.quality_change,
                    out.quality_min.as_ref(),
                    out.quality_max.as_ref(),
                );
                let pi = duals
                    .get(&DualVar::Item(IdWithQuality::new(
                        &out.name,
                        quality_name(ctx, oq),
                    )))
                    .copied()
                    .unwrap_or(0.0);
                // 与品质无关：所有槽位填同一价格
                table.fill(pi);
            } else {
                for (level, slot) in table.iter_mut().enumerate().take(q_count) {
                    let oq = bounded_quality(
                        ctx,
                        level,
                        out.quality_change,
                        out.quality_min.as_ref(),
                        out.quality_max.as_ref(),
                    );
                    *slot = duals
                        .get(&DualVar::Item(IdWithQuality::new(
                            &out.name,
                            quality_name(ctx, oq),
                        )))
                        .copied()
                        .unwrap_or(0.0);
                }
            }
            (out.base, out.prod, out.affected_by_quality, table)
        })
        .collect();

    // 按标量 q 缓存 (v_base, v_prod)
    let mut q_cache: Vec<(u64, f64, f64)> = Vec::new();
    let mut best: Option<(usize, f64)> = None;
    let threshold = -tol * (1.0 + c0.abs());

    for (idx, combo) in group.combos.iter().enumerate() {
        if combo.added {
            continue;
        }
        let eff = &combo.effective;
        let q_bits = norm_f64_bits(eff.quality);
        let (v_base, v_prod) = if let Some(&(_, vb, vp)) = q_cache.iter().find(|&&(k, _, _)| k == q_bits) {
            (vb, vp)
        } else {
            let mut vb = v_const_base;
            let mut vp = v_const_prod;
            if !item_dual_tables.is_empty() {
                let dist = calc_quality_distribution(ctx, eff.quality, group.recipe_quality, q_limit);
                for (base, prod, affected, table) in &item_dual_tables {
                    let expected_pi = if !*affected {
                        table.first().copied().unwrap_or(0.0)
                    } else {
                        dist.iter()
                            .zip(table.iter())
                            .map(|(&prob, &pi)| prob * pi)
                            .sum()
                    };
                    vb += base * expected_pi;
                    vp += prod * expected_pi;
                }
            }
            q_cache.push((q_bits, vb, vp));
            (vb, vp)
        };

        let scale = fast.base_rate * (1.0 + eff.speed).max(0.0);
        let energy_term = pi_elec * fast.base_energy_watts * (1.0 + eff.consumption);
        let out_term = scale * (v_base + eff.productivity * v_prod);
        let in_term = scale * v_in;
        let rc = c0 + energy_term - (out_term - in_term);
        let local_scale = 1.0 + c0.abs() + energy_term.abs() + out_term.abs() + in_term.abs();
        let local_threshold = (-tol * local_scale).min(threshold);

        if rc < local_threshold && best.is_none_or(|(_, best_rc)| rc < best_rc) {
            best = Some((idx, rc));
        }
    }
    best
}

fn price_via_expand(
    group: &PricingGroup,
    combos: &[ComboCandidate],
    ctx: &Context,
    duals: &AIndexMap<DualVar, f64>,
    phase1_zero_cost: bool,
    tol: f64,
) -> Option<(usize, f64)> {
    let mut best: Option<(usize, f64)> = None;
    for (idx, combo) in combos.iter().enumerate() {
        if combo.added {
            continue;
        }
        let mech = group.materialize(idx);
        let exp = metatorio_core::expand::expand([(0usize, &mech)], ctx);
        let Some(var) = exp.variables.first() else {
            continue;
        };
        let cost = if phase1_zero_cost { 0.0 } else { var.cost };
        let mut dot = 0.0;
        let mut abs_dot = 0.0;
        for (k, &v) in &var.flow {
            if let Some(&pi) = duals.get(k) {
                let term = pi * v;
                dot += term;
                abs_dot += term.abs();
            }
        }
        let rc = cost - dot;
        let threshold = -tol * (1.0 + cost.abs() + abs_dot);
        if rc < threshold && best.is_none_or(|(_, best_rc)| rc < best_rc) {
            best = Some((idx, rc));
        }
    }
    best
}

fn price_all_groups(
    groups: &[PricingGroup],
    ctx: &Context,
    duals: &AIndexMap<DualVar, f64>,
    phase1_zero_cost: bool,
    tol: f64,
) -> Vec<(usize, usize, f64)> {
    let mut winners = Vec::new();
    for (g_idx, group) in groups.iter().enumerate() {
        let hit = match group {
            PricingGroup::Recipe(rg) => price_recipe_group(rg, ctx, duals, phase1_zero_cost, tol),
            PricingGroup::Mining(mg) => {
                price_via_expand(group, &mg.combos, ctx, duals, phase1_zero_cost, tol)
            }
        };
        if let Some((c_idx, rc)) = hit {
            winners.push((g_idx, c_idx, rc));
        }
    }
    winners
}

/// 运行基于 HiGHS 延迟列生成的单工厂自动规划，返回 `(选中机制, 最优成本, 统计信息)`。
pub fn solve_autoplan_column_gen(
    store: &PrototypeStore,
    ctx: &Context,
    options: &EnumerateOptions,
    accessibility: &Accessibility,
    build_seed_problem: impl FnOnce(&[Mechanic]) -> SolverData<DualVar, ExpandedVarId>,
    extra_seed_mechanics: &[Mechanic],
    mut on_solving: impl FnMut(usize, usize),
) -> Result<(Vec<Mechanic>, f64, ColumnGenStats), (String, Vec<DualVar>)> {
    let t_enum = Instant::now();
    let (mut active_mechanics, mut groups, raw_full_candidates, total_unique_candidates) =
        enumerate_seed_and_groups(store, ctx, options, accessibility);
    if !extra_seed_mechanics.is_empty() {
        active_mechanics.extend(extra_seed_mechanics.iter().cloned());
    }
    let seed_candidates = active_mechanics.len();
    let enumerate_ms = t_enum.elapsed().as_secs_f64() * 1000.0;

    if active_mechanics.is_empty() {
        return Err(("没有可枚举的机制候选".to_string(), Vec::new()));
    }

    let seed_problem = build_seed_problem(&active_mechanics);
    let mut master = HighsColumnGenMaster::from_seed(&seed_problem)
        .map_err(|e| (format!("初始化列生成主问题失败: {e:?}"), Vec::new()))?;
    on_solving(master.num_cols(), master.num_rows());

    let mut stats = ColumnGenStats {
        seed_candidates,
        total_unique_candidates,
        raw_full_candidates,
        enumerate_ms,
        ..ColumnGenStats::default()
    };

    const MAX_PHASE1_ROUNDS: usize = 30;
    const MAX_PHASE2_ROUNDS: usize = 64;
    const PRICING_TOL: f64 = 1e-7;

    let add_winners = |winners: Vec<(usize, usize, f64)>,
                       groups: &mut [PricingGroup],
                       active_mechanics: &mut Vec<Mechanic>,
                       master: &mut HighsColumnGenMaster<DualVar, ExpandedVarId>|
     -> usize {
        let mut added = 0usize;
        for (g_idx, c_idx, _rc) in winners {
            let group = &mut groups[g_idx];
            match group {
                PricingGroup::Recipe(rg) => rg.combos[c_idx].added = true,
                PricingGroup::Mining(mg) => mg.combos[c_idx].added = true,
            }
            let mech = group.materialize(c_idx);
            let exp = metatorio_core::expand::expand([(0usize, &mech)], ctx);
            let Some(var) = exp.variables.into_iter().next() else {
                continue;
            };
            let new_idx = active_mechanics.len() as u64;
            active_mechanics.push(mech);
            master.add_flow(ExpandedVarId::Mechanic(MechanicId(new_idx)), var.flow, var.cost);
            added += 1;
        }
        added
    };

    // 1) 先试探 Phase II；仅在初始种子集不可行时启动 Phase I（Farkas 定价）
    for _ in 0..MAX_PHASE2_ROUNDS {
        let t_solve = Instant::now();
        let state = master
            .solve_phase2()
            .map_err(|e| (format!("列生成 Phase II 求解失败: {e:?}"), Vec::new()))?;
        stats.master_solve_ms += t_solve.elapsed().as_secs_f64() * 1000.0;

        match state {
            MasterSolveState::Infeasible => {
                // 切入 Phase I 恢复可行性
                let mut recovered = false;
                for _ in 0..MAX_PHASE1_ROUNDS {
                    stats.phase1_rounds += 1;
                    let t_p1 = Instant::now();
                    let p1 = master
                        .solve_phase1()
                        .map_err(|e| (format!("列生成 Phase I 求解失败: {e:?}"), Vec::new()))?;
                    stats.master_solve_ms += t_p1.elapsed().as_secs_f64() * 1000.0;

                    if p1.artificial_sum <= 1e-8 {
                        recovered = true;
                        break;
                    }
                    let t_price = Instant::now();
                    let winners =
                        price_all_groups(&groups, ctx, &p1.effective_duals, true, PRICING_TOL);
                    stats.pricing_ms += t_price.elapsed().as_secs_f64() * 1000.0;

                    if winners.is_empty() {
                        let no_provider: Vec<DualVar> = p1
                            .effective_duals
                            .iter()
                            .filter(|&(_, &y)| y > 1e-6)
                            .map(|(item, _)| item.clone())
                            .collect();
                        return Err((
                            if no_provider.is_empty() {
                            "无可行解（Phase I 列生成已证明完整组合空间不可行）".to_string()
                        } else {
                            {
                            let names: Vec<String> = no_provider
                                .iter()
                                .take(5)
                                .map(|d| match d {
                                    DualVar::Item(id) | DualVar::Entity(id) => id.id.clone(),
                                    DualVar::Fluid { name, .. } => name.clone(),
                                    DualVar::VirtualScience(name) => name.clone(),
                                    DualVar::Electricity => "电力".to_string(),
                                    DualVar::Heat => "热量".to_string(),
                                    _ => "上游流".to_string(),
                                })
                                .collect();
                            format!("无可行解（无供给：{}）", names.join("、"))
                        }
                        },
                            no_provider,
                        ));
                    }
                    stats.added_columns +=
                        add_winners(winners, &mut groups, &mut active_mechanics, &mut master);
                }
                if !recovered {
                    return Err(("无可行解（Phase I 达到轮数上限仍未可行）".to_string(), Vec::new()));
                }
                // Phase I 成功恢复可行性，进入下一轮 Phase II
                continue;
            }
            MasterSolveState::Unbounded => {
                return Err(("求解无界（目标可无限增大）".to_string(), Vec::new()));
            }
            MasterSolveState::Optimal {
                cost,
                primal_flows,
                effective_duals,
                num_cols,
                num_rows,
            } => {
                stats.phase2_rounds += 1;
                stats.final_cols = num_cols;
                stats.final_rows = num_rows;

                let t_price = Instant::now();
                let winners =
                    price_all_groups(&groups, ctx, &effective_duals, false, PRICING_TOL);
                stats.pricing_ms += t_price.elapsed().as_secs_f64() * 1000.0;

                if winners.is_empty() {
                    let filtered =
                        crate::auto_plan::used_candidates(&active_mechanics, primal_flows);
                    let mut used = filtered.used;
                    used.sort_by_key(|m| crate::document::MechanicKind::of(m) as u8);
                    let _ = LpSolver::Highs;
                    return Ok((used, cost, stats));
                }

                stats.added_columns +=
                    add_winners(winners, &mut groups, &mut active_mechanics, &mut master);
            }
        }
    }

    Err(("列生成 Phase II 达到最大轮数仍未收敛".to_string(), Vec::new()))
}


struct LocationPricing {
    location: metatorio_core::Location,
    game: metatorio_core::GameState,
    groups: Vec<PricingGroup>,
}

/// 运行基于 HiGHS 延迟列生成的多星球宇宙工厂自动规划，返回 `(选中机制, 最优成本, 统计信息)`。
pub fn solve_universe_autoplan_column_gen(
    store: &PrototypeStore,
    project_doc: &crate::document::ProjectDocument,
    universe_doc: &crate::document::UniverseDocument,
    accessibility: &Accessibility,
    mut on_solving: impl FnMut(usize, usize),
) -> Result<(Vec<metatorio_core::LocatedMechanic>, f64, ColumnGenStats), (String, Vec<metatorio_core::LocatedDualVar>)> {
    use crate::document::{UniverseDocument, UniverseMechanicEntry};
    use metatorio_core::{
        Accessible, LocalLocatedMechanic, LocatedDualVar, LocatedMechanic, Location,
        OrbitalCargoMechanic,
    };
    use crate::universe::UniverseVarId;

    let t_enum = Instant::now();
    let mut locations: Vec<Location> = store
        .group(PrototypeGroup::Planet)
        .filter(|record| accessibility.is_accessible(&Accessible::Planet(record.name.clone())))
        .map(|record| Location::planet(record.name.clone()))
        .collect();
    locations.push(Location::Space);

    let mut entries: Vec<UniverseMechanicEntry> = Vec::new();
    let mut loc_pricings: Vec<LocationPricing> = Vec::with_capacity(locations.len());
    let mut next_mechanic = 1u64;
    let mut raw_full_candidates = 0usize;
    let mut total_unique_candidates = 0usize;

    for location in &locations {
        crate::solve::check_solve_cancelled().map_err(|e| (e.to_string(), Vec::new()))?;
        let (planet, surface) = crate::universe::environment_for(location);
        let mut game =
            crate::solve::make_game_state_with_accessibility(store, project_doc, accessibility);
        crate::solve::apply_environment_to_game_state(store, &mut game, planet, surface);
        let major_quality = game
            .qualities
            .iter()
            .position(|q| q == &project_doc.settings.major_quality)
            .unwrap_or(0);
        let options = EnumerateOptions {
            alternative_count: project_doc.planning.alternative_count,
            machine_preferences: project_doc.planning.machine_preferences.clone(),
            enumerate_modules: project_doc.planning.enumerate_modules.clone(),
            enumerate_beacons: project_doc.planning.enumerate_beacons.clone(),
            quality_limit: game.max_quality,
            major_quality,
            planet: planet.map(str::to_string),
            surface: surface.map(str::to_string),
            accessibility: Some(accessibility.clone()),
        };

        let (seeds, groups, loc_raw, loc_unique) = {
            let ctx = Context::new(store, &game);
            enumerate_seed_and_groups(store, &ctx, &options, accessibility)
        };
        raw_full_candidates += loc_raw;
        total_unique_candidates += loc_unique;

        for mechanic in seeds {
            let id = MechanicId(next_mechanic);
            next_mechanic += 1;
            entries.push(UniverseMechanicEntry {
                id,
                enabled: true,
                mechanic: LocatedMechanic::Local(LocalLocatedMechanic {
                    location: location.clone(),
                    mechanic,
                }),
            });
        }

        if let Location::Planet(_) = location {
            for record in store.group(PrototypeGroup::Item) {
                for quality in game.qualities.iter().take(game.max_quality + 1) {
                    let item = IdWithQuality::new(&record.name, quality);
                    let weight = store
                        .item_weight(&record.name)
                        .map(|w| w.weight)
                        .unwrap_or(store.weight_constants().default_item_weight);
                    let cargo = OrbitalCargoMechanic {
                        location: location.clone(),
                        item,
                    };
                    let id = MechanicId(next_mechanic);
                    next_mechanic += 1;
                    entries.push(UniverseMechanicEntry {
                        id,
                        enabled: true,
                        mechanic: LocatedMechanic::OrbitalDrop(cargo.clone()),
                    });
                    raw_full_candidates += 1;
                    total_unique_candidates += 1;

                    if weight > 0.0 {
                        let id = MechanicId(next_mechanic);
                        next_mechanic += 1;
                        entries.push(UniverseMechanicEntry {
                            id,
                            enabled: true,
                            mechanic: LocatedMechanic::RocketExport(cargo),
                        });
                        raw_full_candidates += 1;
                        total_unique_candidates += 1;
                    }
                }
            }
        }

        if !groups.is_empty() {
            loc_pricings.push(LocationPricing {
                location: location.clone(),
                game,
                groups,
            });
        }
    }

    if project_doc.planning.preserve_existing_mechanics {
        let mut known: HashSet<String> = entries
            .iter()
            .map(|e| serde_json::to_string(&e.mechanic).unwrap())
            .collect();
        for entry in universe_doc.mechanics.iter().filter(|e| e.enabled) {
            if known.insert(serde_json::to_string(&entry.mechanic).unwrap()) {
                let id = MechanicId(next_mechanic);
                next_mechanic += 1;
                entries.push(UniverseMechanicEntry {
                    id,
                    enabled: true,
                    mechanic: entry.mechanic.clone(),
                });
            }
        }
    }

    if entries.is_empty() {
        return Err(("没有可枚举的宇宙工厂机制候选".to_string(), Vec::new()));
    }

    let seed_candidates = entries.len();
    let enumerate_ms = t_enum.elapsed().as_secs_f64() * 1000.0;

    let temp = UniverseDocument {
        id: universe_doc.id,
        name: universe_doc.name.clone(),
        external_inputs: universe_doc.external_inputs.clone(),
        mechanics: entries,
        targets: universe_doc.targets.clone(),
        target_expressions: universe_doc.target_expressions.clone(),
        strict_source: true,
        strict_sink: universe_doc.strict_sink,
    };

    let (seed_problem, mut var_to_mechanic, mut next_var_id) =
        crate::universe::build_universe_problem(store, project_doc, &temp, accessibility)
            .map_err(|e| (e, Vec::new()))?;
    let mut active_entries = temp.mechanics;

    let mut master = HighsColumnGenMaster::<LocatedDualVar, UniverseVarId>::from_seed(&seed_problem)
        .map_err(|e| (format!("初始化宇宙列生成主问题失败: {e:?}"), Vec::new()))?;
    on_solving(master.num_cols(), master.num_rows());

    let mut stats = ColumnGenStats {
        seed_candidates,
        total_unique_candidates,
        raw_full_candidates,
        enumerate_ms,
        ..ColumnGenStats::default()
    };

    const MAX_PHASE1_ROUNDS: usize = 30;
    const MAX_PHASE2_ROUNDS: usize = 64;
    const PRICING_TOL: f64 = 1e-6;
    let mut prev_cost: Option<f64> = None;
    let mut stagnant_rounds: usize = 0;

    let partition_duals = |duals: &AIndexMap<LocatedDualVar, f64>| -> std::collections::HashMap<Location, AIndexMap<DualVar, f64>> {
        let mut by_loc: std::collections::HashMap<Location, AIndexMap<DualVar, f64>> =
            std::collections::HashMap::new();
        for (located, &pi) in duals {
            if pi != 0.0 {
                by_loc
                    .entry(located.location.clone())
                    .or_default()
                    .insert(located.flow.clone(), pi);
            }
        }
        by_loc
    };

    let price_universe = |loc_pricings: &[LocationPricing],
                          duals: &AIndexMap<LocatedDualVar, f64>,
                          phase1_zero_cost: bool|
     -> Vec<(usize, usize, usize, f64)> {
        let by_loc = partition_duals(duals);
        let mut winners = Vec::new();
        for (loc_idx, lp) in loc_pricings.iter().enumerate() {
            let Some(local_duals) = by_loc.get(&lp.location) else {
                continue;
            };
            let ctx = Context::new(store, &lp.game);
            for (g_idx, c_idx, rc) in
                price_all_groups(&lp.groups, &ctx, local_duals, phase1_zero_cost, PRICING_TOL)
            {
                winners.push((loc_idx, g_idx, c_idx, rc));
            }
        }
        winners
    };

    let mut add_winners = |winners: Vec<(usize, usize, usize, f64)>,
                           loc_pricings: &mut [LocationPricing],
                           active_entries: &mut Vec<UniverseMechanicEntry>,
                           var_to_mechanic: &mut std::collections::HashMap<UniverseVarId, MechanicId>,
                           master: &mut HighsColumnGenMaster<LocatedDualVar, UniverseVarId>|
     -> usize {
        let mut added = 0usize;
        for (loc_idx, g_idx, c_idx, _rc) in winners {
            let lp = &mut loc_pricings[loc_idx];
            let group = &mut lp.groups[g_idx];
            match group {
                PricingGroup::Recipe(rg) => rg.combos[c_idx].added = true,
                PricingGroup::Mining(mg) => mg.combos[c_idx].added = true,
            }
            let mech = group.materialize(c_idx);
            let ctx = Context::new(store, &lp.game);
            let exp = metatorio_core::expand::expand([(0usize, &mech)], &ctx);
            let Some(var) = exp.variables.into_iter().next() else {
                continue;
            };
            let mech_id = MechanicId(next_mechanic);
            next_mechanic += 1;
            active_entries.push(UniverseMechanicEntry {
                id: mech_id,
                enabled: true,
                mechanic: LocatedMechanic::Local(LocalLocatedMechanic {
                    location: lp.location.clone(),
                    mechanic: mech,
                }),
            });
            let var_id = UniverseVarId(next_var_id);
            next_var_id += 1;
            var_to_mechanic.insert(var_id, mech_id);
            master.add_flow(
                var_id,
                crate::universe::locate_flow(&lp.location, var.flow),
                var.cost,
            );
            added += 1;
        }
        added
    };

    for _ in 0..MAX_PHASE2_ROUNDS {
        crate::solve::check_solve_cancelled().map_err(|e| (e.to_string(), Vec::new()))?;
        let t_solve = Instant::now();
        let state = master
            .solve_phase2()
            .map_err(|e| (format!("宇宙列生成 Phase II 求解失败: {e:?}"), Vec::new()))?;
        stats.master_solve_ms += t_solve.elapsed().as_secs_f64() * 1000.0;

        match state {
            MasterSolveState::Infeasible => {
                let mut recovered = false;
                for _ in 0..MAX_PHASE1_ROUNDS {
                    crate::solve::check_solve_cancelled().map_err(|e| (e.to_string(), Vec::new()))?;
                    stats.phase1_rounds += 1;
                    let t_p1 = Instant::now();
                    let p1 = master
                        .solve_phase1()
                        .map_err(|e| (format!("宇宙列生成 Phase I 求解失败: {e:?}"), Vec::new()))?;
                    stats.master_solve_ms += t_p1.elapsed().as_secs_f64() * 1000.0;

                    if p1.artificial_sum <= 1e-8 {
                        recovered = true;
                        break;
                    }
                    let t_price = Instant::now();
                    let winners = price_universe(&loc_pricings, &p1.effective_duals, true);
                    stats.pricing_ms += t_price.elapsed().as_secs_f64() * 1000.0;

                    if winners.is_empty() {
                        let no_provider: Vec<LocatedDualVar> = p1
                            .effective_duals
                            .iter()
                            .filter(|&(_, &y)| y > 1e-6)
                            .map(|(item, _)| item.clone())
                            .collect();
                        let desc = if no_provider.is_empty() {
                            "无可行解（Phase I 列生成已证明宇宙组合空间不可行）".to_string()
                        } else {
                            format!("无可行解（无供给：共 {} 项）", no_provider.len())
                        };
                        return Err((desc, no_provider));
                    }
                    stats.added_columns += add_winners(
                        winners,
                        &mut loc_pricings,
                        &mut active_entries,
                        &mut var_to_mechanic,
                        &mut master,
                    );
                }
                if !recovered {
                    return Err((
                        "无可行解（宇宙 Phase I 达到轮数上限仍未可行）".to_string(),
                        Vec::new(),
                    ));
                }
                continue;
            }
            MasterSolveState::Unbounded => {
                return Err(("求解无界（目标可无限增大）".to_string(), Vec::new()));
            }
            MasterSolveState::Optimal {
                cost,
                primal_flows,
                effective_duals,
                num_cols,
                num_rows,
            } => {
                stats.phase2_rounds += 1;
                stats.final_cols = num_cols;
                stats.final_rows = num_rows;

                if let Some(prev) = prev_cost {
                    let rel_improve = (prev - cost) / prev.abs().max(1.0);
                    if rel_improve < 1e-9 {
                        stagnant_rounds += 1;
                    } else {
                        stagnant_rounds = 0;
                    }
                }
                prev_cost = Some(cost);

                let t_price = Instant::now();
                let winners = if stagnant_rounds >= 2 {
                    Vec::new()
                } else {
                    price_universe(&loc_pricings, &effective_duals, false)
                };
                stats.pricing_ms += t_price.elapsed().as_secs_f64() * 1000.0;

                if winners.is_empty() {
                    let mut seen_ids: HashSet<MechanicId> = HashSet::new();
                    let mut used: Vec<LocatedMechanic> = Vec::new();
                    for (var_id, amount) in primal_flows {
                        if amount == 0.0 {
                            continue;
                        }
                        if let Some(&mech_id) = var_to_mechanic.get(&var_id)
                            && seen_ids.insert(mech_id)
                            && let Some(index) = mech_id.0.checked_sub(1).map(|i| i as usize)
                            && let Some(entry) = active_entries.get(index)
                        {
                            used.push(entry.mechanic.clone());
                        }
                    }
                    return Ok((used, cost, stats));
                }

                let added_now = add_winners(
                    winners,
                    &mut loc_pricings,
                    &mut active_entries,
                    &mut var_to_mechanic,
                    &mut master,
                );
                stats.added_columns += added_now;
                if std::env::var_os("METATORIO_CG_TRACE").is_some() {
                    eprintln!(
                        "[UNIV-CG] round={} cost={:.4} cols={} rows={} added={} solve_ms={:.1} price_ms={:.1}",
                        stats.phase2_rounds, cost, num_cols, num_rows, added_now, stats.master_solve_ms, stats.pricing_ms
                    );
                }
            }
        }
    }

    Err(("宇宙列生成 Phase II 达到最大轮数仍未收敛".to_string(), Vec::new()))
}
