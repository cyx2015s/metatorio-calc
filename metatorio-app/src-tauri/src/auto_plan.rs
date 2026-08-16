//! 自动规划：完整状态空间枚举（复刻原版 auto.rs + 各机制 auto_populate）。
//!
//! 为每种机制枚举候选实例（配方 × 机器 × 插件组合 × 信塔配置 × 品质），
//! 一次构建 LP 求解，保留被选中的实例。相比旧实现的"迭代贪婪"，这是
//! 原版用户实际使用的"枚举全部组合取最优"。

use metatorio_core::{Context, IdWithQuality, Mechanic, ModuleConfig, NORMAL_QUALITY};
use metatorio_data::store::{PrototypeGroup, PrototypeRecord, PrototypeStore};
use metatorio_data::types::{EffectType, EffectTypeLimitation};
use metatorio_data::{
    BoilerComponent, BurnerGeneratorComponent, CraftingMachineComponent, EntityComponent,
    FluidComponent, GeneratorComponent, ItemComponent, MiningDrillComponent, ModuleComponent,
    ReactorComponent, RecipeComponent, ResourceEntityComponent,
};

/// 枚举候选配置所需的项目级参数。
pub struct EnumerateOptions {
    pub alternative_count: usize,
    pub machine_preferences: Vec<IdWithQuality>,
    pub enumerate_modules: Vec<IdWithQuality>,
    pub enumerate_beacons: Vec<metatorio_runtime::document::AutoBeaconPlan>,
    /// 项目品质上限（等级索引）。
    pub quality_limit: usize,
    /// 机器/设备使用的品质等级（工厂主品质）。
    pub major_quality: usize,
    /// 当前工厂的星球/地表（表面条件过滤；种子可用性）。
    pub planet: Option<String>,
    pub surface: Option<String>,
}

pub fn enumerate_all(
    store: &PrototypeStore,
    ctx: &Context,
    options: &EnumerateOptions,
) -> Vec<Mechanic> {
    let mut out = Vec::new();
    enumerate_recipes(store, ctx, options, &mut out);
    enumerate_mining(store, ctx, options, &mut out);
    enumerate_simple(store, ctx, options, &mut out);
    enumerate_energy(store, ctx, options, &mut out);
    out
}

/// 当前工厂的表面属性（planet/surface 规则见 metatorio_runtime::planet）。
fn surface_properties(store: &PrototypeStore, options: &EnumerateOptions) -> Option<std::collections::BTreeMap<String, f64>> {
    metatorio_runtime::planet::surface_properties_of(
        store,
        options.planet.as_deref(),
        options.surface.as_deref(),
    )
}

// ── 通用工具 ──────────────────────────────────────────────────────

fn quality_name(ctx: &Context, level: usize) -> String {
    ctx.game
        .qualities
        .get(level)
        .cloned()
        .or_else(|| ctx.prototype.quality_order().get(level).cloned())
        .unwrap_or_else(|| NORMAL_QUALITY.to_string())
}

fn fluid_record<'a>(ctx: &'a Context, name: &str) -> Option<&'a FluidComponent> {
    ctx.prototype
        .get(PrototypeGroup::Fluid, name)
        .and_then(|record| record.component())
}

fn machine_fits_recipe(machine: &CraftingMachineComponent, recipe: &RecipeComponent) -> bool {
    let required = effective_recipe_categories(recipe);
    if required.is_empty() {
        return true;
    }
    machine
        .crafting_categories
        .iter()
        .any(|available| required.contains(available))
}

fn effective_recipe_categories(recipe: &RecipeComponent) -> Vec<String> {
    let categories = recipe.categories.clone().unwrap_or_default();
    if categories.is_empty() {
        vec!["crafting".to_string()]
    } else {
        categories
    }
}

/// 配方/机器的插件类别与效果限制（复刻 egui collect_module_limitations）。
fn module_allowed(
    module: &ModuleComponent,
    machine_categories: &Option<Vec<String>>,
    machine_allowed_effects: &Option<EffectTypeLimitation>,
    recipe: Option<&RecipeComponent>,
) -> bool {
    if let Some(categories) = machine_categories {
        if !categories.is_empty() && !categories.contains(&module.category) {
            return false;
        }
    }
    let recipe_allowed = |kind: EffectType, recipe_allow: bool| {
        recipe_allow && machine_allowed_effects.is_none_or(|limits| limits[kind])
    };
    let effect = &module.effect;
    if effect.speed != 0.0
        && !recipe_allowed(EffectType::Speed, recipe.is_none_or(|r| r.allow_speed))
    {
        return false;
    }
    if effect.productivity != 0.0
        && !recipe_allowed(
            EffectType::Productivity,
            recipe.is_none_or(|r| r.allow_productivity),
        )
    {
        return false;
    }
    if effect.quality != 0.0
        && !recipe_allowed(EffectType::Quality, recipe.is_none_or(|r| r.allow_quality))
    {
        return false;
    }
    if effect.consumption != 0.0
        && !recipe_allowed(
            EffectType::Consumption,
            recipe.is_none_or(|r| r.allow_consumption),
        )
    {
        return false;
    }
    if effect.pollution != 0.0
        && !recipe_allowed(EffectType::Pollution, recipe.is_none_or(|r| r.allow_pollution))
    {
        return false;
    }
    true
}

/// 选取至多 alternative_count 台不同机器：项目偏好优先，其次分数（速度）。
fn pick_machines<F, S>(
    store: &PrototypeStore,
    prefs: &[IdWithQuality],
    alternative_count: usize,
    matches: F,
    score: S,
) -> Vec<String>
where
    F: Fn(&PrototypeRecord) -> bool,
    S: Fn(&PrototypeRecord) -> f64,
{
    let mut candidates: Vec<(&PrototypeRecord, f64)> = store
        .group(PrototypeGroup::Entity)
        .filter(|record| matches(record))
        .map(|record| (record, score(record)))
        .collect();
    let mut out: Vec<String> = Vec::new();
    for pref in prefs {
        if let Some(index) = candidates.iter().position(|(record, _)| record.name == pref.id) {
            let record = candidates.remove(index).0;
            if !out.contains(&record.name) {
                out.push(record.name.clone());
            }
        }
    }
    candidates.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.name.cmp(&b.0.name))
    });
    for (record, _) in candidates {
        if out.len() >= alternative_count.max(1).min(3) {
            break;
        }
        if !out.contains(&record.name) {
            out.push(record.name.clone());
        }
    }
    out
}

/// 弱组合：长度为 `parts`、和为 `sum` 的非负整数向量（含零）。
fn compositions(parts: usize, sum: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut current = vec![0usize; parts];
    fn rec(parts: usize, index: usize, remaining: usize, current: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if index == parts - 1 {
            current[index] = remaining;
            out.push(current.clone());
            return;
        }
        for value in 0..=remaining {
            current[index] = value;
            rec(parts, index + 1, remaining - value, current, out);
        }
    }
    rec(parts, 0, sum, &mut current, &mut out);
    out
}

/// 插件组合枚举（复刻 egui recipe.rs:1320-1345 的状态空间降级策略）。
/// 返回 (每插件数量向量, 每插件重复系数 dup)。
fn module_combinations(
    allowed_modules: usize,
    module_slots: usize,
    quality_involved: bool,
) -> Vec<(Vec<usize>, usize)> {
    if allowed_modules == 0 {
        return vec![(Vec::new(), 1)];
    }
    if allowed_modules > 5 {
        // 只枚举单一插件的重复配置，避免状态空间爆炸
        let dup = module_slots.min(24);
        return (0..allowed_modules)
            .map(|module| {
                let mut comb = vec![0usize; allowed_modules];
                comb[module] = 1;
                (comb, dup)
            })
            .collect();
    }
    let parts = if quality_involved {
        allowed_modules + 1
    } else {
        allowed_modules.max(1)
    };
    let mut slots = module_slots;
    if allowed_modules > 2 && module_slots > 16 {
        slots = 16;
    }
    if allowed_modules > 1 && module_slots > 24 {
        slots = 24;
    }
    compositions(parts, slots)
        .into_iter()
        .map(|comb| (comb, 1usize))
        .collect()
}

fn push_with_beacons(
    out: &mut Vec<Mechanic>,
    base: Mechanic,
    modules: Vec<IdWithQuality>,
    beacons: &[metatorio_runtime::document::AutoBeaconPlan],
) {
    let with_beacons = |module_config: ModuleConfig| match &base {
        Mechanic::Recipe(mechanic) => Mechanic::Recipe(metatorio_core::RecipeMechanic {
            module_config,
            ..mechanic.clone()
        }),
        Mechanic::Mining(mechanic) => Mechanic::Mining(metatorio_core::MiningMechanic {
            module_config,
            ..mechanic.clone()
        }),
        _ => base.clone(),
    };
    // 无信塔版本
    out.push(with_beacons(ModuleConfig {
        modules: modules.clone(),
        beacons: Vec::new(),
    }));
    // 每个枚举信塔配置额外创建一个变体
    for plan in beacons {
        out.push(with_beacons(ModuleConfig {
            modules: modules.clone(),
            beacons: plan.module_config.beacons.clone(),
        }));
    }
}

// ── 配方 ──────────────────────────────────────────────────────────

fn enumerate_recipes(
    store: &PrototypeStore,
    ctx: &Context,
    options: &EnumerateOptions,
    out: &mut Vec<Mechanic>,
) {
    let quality_range = options.quality_limit + 1;
    let major_quality = quality_name(ctx, options.major_quality);
    let beacons = &options.enumerate_beacons;
    // 表面条件：配方与机器都要满足当前星球/地表属性（自动规划才校验，
    // 手动模式认为所有配方可用）。
    let properties = surface_properties(store, options);
    for record in store.group(PrototypeGroup::Recipe) {
        let Some(recipe) = record.component::<RecipeComponent>() else {
            continue;
        };
        if let Some(properties) = &properties {
            if !metatorio_runtime::planet::surface_condition_satisfied(
                store,
                &recipe.surface_conditions,
                properties,
            ) {
                continue;
            }
        }
        // 有物品原料的配方按品质展开；纯流体配方只有 normal。
        let has_item_ingredient = recipe.ingredients.iter().any(|ingredient| {
            matches!(ingredient, metatorio_data::types::Ingredient::Item(_))
        });
        let recipe_quality_range = if has_item_ingredient { quality_range } else { 1 };
        let machines = pick_machines(
            store,
            &options.machine_preferences,
            options.alternative_count,
            |record| {
                record
                    .component::<CraftingMachineComponent>()
                    .is_some_and(|machine| machine_fits_recipe(machine, recipe))
            },
            |record| {
                record
                    .component::<CraftingMachineComponent>()
                    .map(|machine| machine.crafting_speed)
                    .unwrap_or(0.0)
            },
        );
        for machine_name in machines {
            let Some(machine_record) = store.get(PrototypeGroup::Entity, &machine_name) else {
                continue;
            };
            // 机器表面条件过滤
            if let Some(properties) = &properties {
                if let Some(entity) = machine_record.component::<EntityComponent>() {
                    if !metatorio_runtime::planet::surface_condition_satisfied(
                        store,
                        &entity.surface_conditions,
                        properties,
                    ) {
                        continue;
                    }
                }
            }
            let Some(machine) = machine_record.component::<CraftingMachineComponent>() else {
                continue;
            };
            let allowed_modules: Vec<IdWithQuality> = options
                .enumerate_modules
                .iter()
                .filter(|module_name| {
                    store
                        .get(PrototypeGroup::Item, &module_name.id)
                        .and_then(|record| record.component::<ModuleComponent>())
                        .is_some_and(|module| {
                            module_allowed(
                                module,
                                &machine.allowed_module_categories,
                                &machine.allowed_effects,
                                Some(recipe),
                            )
                        })
                })
                .cloned()
                .collect();
            let quality_involved = allowed_modules.iter().any(|module_name| {
                store
                    .get(PrototypeGroup::Item, &module_name.id)
                    .and_then(|record| record.component::<ModuleComponent>())
                    .is_some_and(|module| module.effect.quality > 0.0)
            });
            let module_slots = machine.module_slots.map(|slots| slots as usize).unwrap_or(0);
            let combos = module_combinations(allowed_modules.len(), module_slots, quality_involved);
            for (comb, dup) in combos {
                for quality in 0..recipe_quality_range {
                    let mut modules = Vec::new();
                    for (module_id, module) in allowed_modules.iter().enumerate() {
                        for _ in 0..(comb.get(module_id).copied().unwrap_or(0) * dup) {
                            modules.push(module.clone());
                        }
                    }
                    let base = Mechanic::Recipe(metatorio_core::RecipeMechanic {
                        recipe: IdWithQuality::new(record.name.clone(), quality_name(ctx, quality)),
                        machine: IdWithQuality::new(machine_name.clone(), major_quality.clone()),
                        module_config: ModuleConfig::default(),
                        fuel: None,
                        fuel_temperature: None,
                    });
                    push_with_beacons(out, base, modules, beacons);
                }
            }
        }
    }
}

// ── 采矿 ──────────────────────────────────────────────────────────

fn enumerate_mining(
    store: &PrototypeStore,
    ctx: &Context,
    options: &EnumerateOptions,
    out: &mut Vec<Mechanic>,
) {
    let quality_range = options.quality_limit + 1;
    let major_quality = quality_name(ctx, options.major_quality);
    let beacons = &options.enumerate_beacons;
    let properties = surface_properties(store, options);
    for record in store.group(PrototypeGroup::Entity) {
        if record.type_ != "resource" {
            continue;
        }
        let Some(resource) = record.component::<ResourceEntityComponent>() else {
            continue;
        };
        let category = if resource.category.is_empty() {
            "basic-solid".to_string()
        } else {
            resource.category.clone()
        };
        let machines = pick_machines(
            store,
            &options.machine_preferences,
            options.alternative_count,
            |drill_record| {
                drill_record
                    .component::<MiningDrillComponent>()
                    .is_some_and(|drill| drill.resource_categories.contains(&category))
            },
            |_| 0.0,
        );
        for machine_name in machines {
            let Some(drill_record) = store.get(PrototypeGroup::Entity, &machine_name) else {
                continue;
            };
            // 采矿机表面条件过滤
            if let Some(properties) = &properties {
                if let Some(entity) = drill_record.component::<EntityComponent>() {
                    if !metatorio_runtime::planet::surface_condition_satisfied(
                        store,
                        &entity.surface_conditions,
                        properties,
                    ) {
                        continue;
                    }
                }
            }
            let Some(drill) = drill_record.component::<MiningDrillComponent>() else {
                continue;
            };
            let allowed_modules: Vec<IdWithQuality> = options
                .enumerate_modules
                .iter()
                .filter(|module_name| {
                    store
                        .get(PrototypeGroup::Item, &module_name.id)
                        .and_then(|record| record.component::<ModuleComponent>())
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
                .collect();
            let quality_involved = allowed_modules.iter().any(|module_name| {
                store
                    .get(PrototypeGroup::Item, &module_name.id)
                    .and_then(|record| record.component::<ModuleComponent>())
                    .is_some_and(|module| module.effect.quality > 0.0)
            });
            let module_slots = drill.module_slots.map(|slots| slots as usize).unwrap_or(0);
            let combos = module_combinations(allowed_modules.len(), module_slots, quality_involved);
            for (comb, dup) in combos {
                for quality in 0..quality_range {
                    let mut modules = Vec::new();
                    for (module_id, module) in allowed_modules.iter().enumerate() {
                        for _ in 0..(comb.get(module_id).copied().unwrap_or(0) * dup) {
                            modules.push(module.clone());
                        }
                    }
                    let base = Mechanic::Mining(metatorio_core::MiningMechanic {
                        resource: record.name.clone(),
                        machine: IdWithQuality::new(machine_name.clone(), major_quality.clone()),
                        module_config: ModuleConfig::default(),
                        fuel: None,
                        fuel_temperature: None,
                    });
                    // 采矿按资源品质展开（矿藏实体无品质，品质作用于产出）
                    let _ = quality;
                    push_with_beacons(out, base, modules, beacons);
                }
            }
        }
    }
}

// ── 简单机制（无机器/插件） ───────────────────────────────────────

fn enumerate_simple(
    store: &PrototypeStore,
    ctx: &Context,
    options: &EnumerateOptions,
    out: &mut Vec<Mechanic>,
) {
    let quality_range = options.quality_limit + 1;
    // 腐坏 / 种植 / 物品燃料 / 火箭发射
    for record in store.group(PrototypeGroup::Item) {
        let Some(item) = record.component::<ItemComponent>() else {
            continue;
        };
        let has_spoil = item.spoil_result.as_deref().is_some_and(|r| !r.is_empty());
        let has_plant = item.plant_result.as_deref().is_some_and(|r| !r.is_empty());
        let has_fuel = item.fuel_value().amount > 0.0;
        let has_launch = !item.rocket_launch_products.is_empty();
        if !has_spoil && !has_plant && !has_fuel && !has_launch {
            continue;
        }
        for quality in 0..quality_range {
            let id = IdWithQuality::new(record.name.clone(), quality_name(ctx, quality));
            if has_spoil {
                out.push(Mechanic::Spoil(metatorio_core::SpoilMechanic { item: id.clone() }));
            }
            // 种子可用性：种植物要求的 tile 与星球生成 tile 交集，回退 default_import_location
            if has_plant {
                let plant_entity = item.plant_result.as_deref().unwrap_or("");
                let available = metatorio_runtime::planet::seed_available_on_planet(
                    store,
                    item,
                    plant_entity,
                    options.planet.as_deref(),
                );
                if available {
                    out.push(Mechanic::Plant(metatorio_core::PlantMechanic { seed: id.clone() }));
                }
            }
            if has_fuel {
                out.push(Mechanic::ItemFuel(metatorio_core::ItemFuelMechanic { item: id.clone() }));
            }
            if has_launch {
                out.push(Mechanic::ItemLaunch(metatorio_core::ItemLaunchMechanic {
                    item: id.clone(),
                    weight_mode: false,
                }));
            }
        }
    }
    // 流体燃料 / 流体热
    for record in store.group(PrototypeGroup::Fluid) {
        let Some(fluid) = record.component::<FluidComponent>() else {
            continue;
        };
        if fluid.fuel_value().amount > 0.0 {
            out.push(Mechanic::FluidFuel(metatorio_core::FluidFuelMechanic {
                fluid: record.name.clone(),
                temperature: Some(fluid.default_temperature as i32),
            }));
        }
        // 提热流体：用最高温度（高于默认温度才产热）
        let temperature = fluid
            .max_temperature
            .unwrap_or(fluid.default_temperature);
        if temperature > fluid.default_temperature {
            out.push(Mechanic::FluidHeat(metatorio_core::FluidHeatMechanic {
                fluid: record.name.clone(),
                temperature: Some(temperature as i32),
            }));
        }
    }
}

// ── 能量机制（发电机/锅炉/反应堆） ───────────────────────────────

fn enumerate_energy(
    store: &PrototypeStore,
    ctx: &Context,
    options: &EnumerateOptions,
    out: &mut Vec<Mechanic>,
) {
    let major_quality = quality_name(ctx, options.major_quality);
    for record in store.group(PrototypeGroup::Entity) {
        if let Some(generator) = record.component::<GeneratorComponent>() {
            let Some(fluid) = generator.fluid_box.filter.clone() else {
                continue;
            };
            let temperature = if generator.maximum_temperature > 0.0 {
                generator.maximum_temperature
            } else {
                fluid_record(ctx, &fluid).map(|f| f.default_temperature).unwrap_or(0.0)
            };
            out.push(Mechanic::Generator(metatorio_core::GeneratorMechanic {
                generator: IdWithQuality::new(record.name.clone(), major_quality.clone()),
                fluid,
                temperature: Some(temperature as i32),
            }));
        }
        if let Some(_burner) = record.component::<BurnerGeneratorComponent>() {
            // 烧燃料发电机：无流体可枚举，交给用户手动配置
        }
        if let Some(boiler) = record.component::<BoilerComponent>() {
            let Some(fluid) = boiler.fluid_box.filter.clone() else {
                continue;
            };
            out.push(Mechanic::Boiler(metatorio_core::BoilerMechanic {
                boiler: IdWithQuality::new(record.name.clone(), major_quality.clone()),
                fluid,
                temperature: None,
                fuel: None,
                fuel_temperature: None,
            }));
        }
        if let Some(_reactor) = record.component::<ReactorComponent>() {
            out.push(Mechanic::Reactor(metatorio_core::ReactorMechanic {
                reactor: IdWithQuality::new(record.name.clone(), major_quality.clone()),
                fuel: None,
                neighbours: 3,
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use metatorio_core::context::GameState;

    #[test]
    fn enumerates_demo_recipe_candidates() {
        let dump: serde_json::Value =
            serde_json::from_str(include_str!("../dumps/demo_dump.json")).unwrap();
        let store = PrototypeStore::load(&dump).unwrap();
        let game = GameState::default();
        let ctx = Context::new(&store, &game);
        let options = EnumerateOptions {
            alternative_count: 1,
            machine_preferences: Vec::new(),
            enumerate_modules: Vec::new(),
            enumerate_beacons: Vec::new(),
            quality_limit: 0,
            major_quality: 0,
            planet: None,
            surface: None,
        };
        let candidates = enumerate_all(&store, &ctx, &options);
        // demo 只有 iron-plate 配方 + 一个组装机 → 至少一个配方候选
        assert!(
            candidates.iter().any(|mechanic| matches!(mechanic, Mechanic::Recipe(_))),
            "应枚举出配方候选：{candidates:?}"
        );
    }
}
