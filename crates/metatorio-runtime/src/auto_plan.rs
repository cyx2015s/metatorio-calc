//! 自动规划：完整状态空间枚举（复刻原版 auto.rs + 各机制 auto_populate）。
//!
//! 为每种机制枚举候选实例（配方 × 机器 × 插件组合 × 信塔配置 × 品质），
//! 一次构建 LP 求解，保留被选中的实例。相比旧实现的"迭代贪婪"，这是
//! 原版用户实际使用的"枚举全部组合取最优"。

use metatorio_core::{
    Accessibility, Accessible, Context, IdWithQuality, Mechanic, ModuleConfig, NORMAL_QUALITY,
};
use metatorio_data::store::{PrototypeGroup, PrototypeRecord, PrototypeStore};
use metatorio_data::types::{EffectType, EffectTypeLimitation, EnergySource, Modifier};
use metatorio_data::{
    AccumulatorComponent, AssemblingMachineComponent, BoilerComponent, BurnerGeneratorComponent,
    CraftingMachineComponent, EntityComponent, FluidComponent, GeneratorComponent, ItemComponent,
    MiningDrillComponent, ModuleComponent, ReactorComponent, RecipeComponent,
    ResourceEntityComponent, SolarPanelComponent, TechnologyComponent,
};
use crate::solve::ExpandedVarId;

/// 把求解结果中选中的流映射回候选机制。
///
/// 过滤掉零成本转换流的辅助变量（`MechanicId(u64::MAX)`，非真实机制）
/// 和用量低于阈值的流；剩余流一定落在候选索引范围内（`[]` 自带越界检查）。
///
/// 判断"用量 > 阈值"用内部缩放值 `amount / scale`（剔除逐变量 Ruiz
/// 缩放差异），避免单次产出大的配方因表观量小被误判为未使用。
pub fn used_candidates<T: Clone>(
    candidates: &[T],
    prim: impl IntoIterator<Item = (ExpandedVarId, f64)>,
    prim_scale: impl IntoIterator<Item = (ExpandedVarId, f64)>,
) -> Vec<T> {
    let scales: std::collections::HashMap<ExpandedVarId, f64> =
        prim_scale.into_iter().collect();
    prim.into_iter()
        .filter(|(id, amount)| {
            if id.mechanic.0 == u64::MAX {
                return false;
            }
            let scale = scales.get(id).copied().unwrap_or(1.0).max(1e-12);
            *amount / scale > 1e-9
        })
        .map(|(id, _)| candidates[id.mechanic.0 as usize].clone())
        .collect()
}

/// 枚举候选配置所需的项目级参数。
pub struct EnumerateOptions {
    pub alternative_count: usize,
    pub machine_preferences: Vec<IdWithQuality>,
    pub enumerate_modules: Vec<IdWithQuality>,
    pub enumerate_beacons: Vec<crate::document::AutoBeaconPlan>,
    /// 项目品质上限（等级索引）。
    pub quality_limit: usize,
    /// 机器/设备使用的品质等级（工厂主品质）。
    pub major_quality: usize,
    /// 当前工厂的星球/地表（表面条件过滤；种子可用性）。
    pub planet: Option<String>,
    pub surface: Option<String>,
    /// 项目可达性（机器候选按此过滤：不可达机器不枚举）。
    /// `None` = 不做机器可达性过滤（测试/无项目可达性时）。
    pub accessibility: Option<metatorio_core::Accessibility>,
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
    crate::planet::surface_properties_of(
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

use crate::prototype::effective_recipe_categories;

/// 配方/机器的插件类别与效果限制（复刻 egui collect_module_limitations）。
///
/// 规则：
/// - `machine_categories`（allowed_module_categories）：非空时必须包含
///   插件类别（空/None = 全部支持）；
/// - `machine_allowed_effects`（allowed_effects）：插件**正面**效果属性
///   必须被机器允许。正面方向按效果类型区分：速度/产能/品质越高越好
///   （> 0 触发鉴权），污染/能耗越低越好（< 0 即降低污染/能耗才是正面，
///   触发鉴权）。反向效果（如品质惩罚、增加能耗）不限制；配方机制还
///   叠加配方的 allow_speed/allow_productivity/... 开关。
///
/// 自动规划枚举与机制卡手动插件选择共用此鉴权。
pub fn module_allowed(
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
    if effect.speed > 0.0
        && !recipe_allowed(EffectType::Speed, recipe.is_none_or(|r| r.allow_speed))
    {
        return false;
    }
    if effect.productivity > 0.0
        && !recipe_allowed(
            EffectType::Productivity,
            recipe.is_none_or(|r| r.allow_productivity),
        )
    {
        return false;
    }
    if effect.quality > 0.0
        && !recipe_allowed(EffectType::Quality, recipe.is_none_or(|r| r.allow_quality))
    {
        return false;
    }
    // 能耗/污染：正面 = 降低（< 0），即减能耗/减污染需要许可。
    if effect.consumption < 0.0
        && !recipe_allowed(
            EffectType::Consumption,
            recipe.is_none_or(|r| r.allow_consumption),
        )
    {
        return false;
    }
    if effect.pollution < 0.0
        && !recipe_allowed(EffectType::Pollution, recipe.is_none_or(|r| r.allow_pollution))
    {
        return false;
    }
    true
}

/// 选取至多 alternative_count 台不同机器：项目偏好优先（**保留用户指定
/// 的品质**），其次分数（复刻原版 measure_crafter：速度/碰撞箱面积 ×
/// (1+基础效果速度) × (1+基础效果产能×2) × 电动机器×8 × (1+插件槽)）。
/// 自动枚举出的机器品质 = 主品质；与用户偏好重复时优先用户的品质。
fn pick_machines<F, S>(
    store: &PrototypeStore,
    prefs: &[IdWithQuality],
    alternative_count: usize,
    major_quality: &str,
    matches: F,
    score: S,
) -> Vec<IdWithQuality>
where
    F: Fn(&PrototypeRecord) -> bool,
    S: Fn(&PrototypeRecord) -> f64,
{
    let mut candidates: Vec<(&PrototypeRecord, f64)> = store
        .group(PrototypeGroup::Entity)
        .filter(|record| matches(record))
        .map(|record| (record, score(record)))
        .collect();
    let mut out: Vec<IdWithQuality> = Vec::new();
    let contains_name = |out: &[IdWithQuality], name: &str| out.iter().any(|m| m.id == name);
    // 用户偏好优先（带品质）；同一台机器只保留一次
    for pref in prefs {
        if let Some(index) = candidates.iter().position(|(record, _)| record.name == pref.id) {
            let record = candidates.remove(index).0;
            if !contains_name(&out, &record.name) {
                out.push(pref.clone());
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
        if !contains_name(&out, &record.name) {
            out.push(IdWithQuality::new(record.name.clone(), major_quality.to_string()));
        }
    }
    out
}

/// 机器打分（复刻原版 recipe.rs::measure_crafter）：
/// 速度 / 碰撞箱面积 × (1+基础效果速度) × (1+基础效果产能×2) × 电动×8 × (1+插件槽)。
fn crafter_score(machine: &CraftingMachineComponent, entity: Option<&EntityComponent>) -> f64 {
    let area = entity
        .and_then(|e| e.collision_box.as_ref())
        .map_or(25.0, |bb| (bb.1 .0 - bb.0 .0).abs() * (bb.1 .1 - bb.0 .1).abs());
    let mut score = machine.crafting_speed / area;
    if let Some(effect_receiver) = &machine.effect_receiver {
        if let Some(base) = &effect_receiver.base_effect {
            score *= 1.0 + base.speed;
            score *= 1.0 + (base.productivity * 2.0);
        }
    }
    if matches!(machine.energy_source, EnergySource::Electric(_)) {
        score *= 8.0;
    }
    score *= 1.0 + machine.module_slots.unwrap_or(0) as f64;
    score
}

/// 采矿机打分（复刻原版 mining.rs::measure_miner）：mining_speed 同构。
fn miner_score(miner: &MiningDrillComponent, entity: Option<&EntityComponent>) -> f64 {
    let area = entity
        .and_then(|e| e.collision_box.as_ref())
        .map_or(25.0, |bb| (bb.1 .0 - bb.0 .0).abs() * (bb.1 .1 - bb.0 .1).abs());
    let mut score = miner.mining_speed / area;
    if let Some(effect_receiver) = &miner.effect_receiver {
        if let Some(base) = &effect_receiver.base_effect {
            score *= 1.0 + base.speed;
            score *= 1.0 + (base.productivity * 2.0);
        }
    }
    if matches!(miner.energy_source, EnergySource::Electric(_)) {
        score *= 8.0;
    }
    score *= 1.0 + miner.module_slots.unwrap_or(0) as f64;
    score
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
    beacons: &[crate::document::AutoBeaconPlan],
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
            if !crate::planet::surface_condition_satisfied(
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
            &major_quality,
            |record| {
                record
                    .component::<CraftingMachineComponent>()
                    .is_some_and(|machine| machine_fits_recipe(machine, recipe))
            },
            |record| {
                record
                    .component::<CraftingMachineComponent>()
                    .zip(record.component::<EntityComponent>())
                    .map(|(machine, entity)| crafter_score(machine, Some(entity)))
                    .unwrap_or(0.0)
            },
        );
        let mut kept_any = false;
        for machine in &machines {
            let machine_name = &machine.id;
            let Some(machine_record) = store.get(PrototypeGroup::Entity, machine_name) else {
                continue;
            };
            // 机器表面条件过滤
            if let Some(properties) = &properties {
                if let Some(entity) = machine_record.component::<EntityComponent>() {
                    if !crate::planet::surface_condition_satisfied(
                        store,
                        &entity.surface_conditions,
                        properties,
                    ) {
                        continue;
                    }
                }
            }
            // 机器可达性过滤：当前项目科技未解锁的机器不枚举。
            if let Some(accessibility) = &options.accessibility {
                if !accessibility.is_accessible(&metatorio_core::Accessible::Entity(
                    machine_name.clone(),
                )) {
                    continue;
                }
            }
            let Some(machine_component) = machine_record.component::<CraftingMachineComponent>()
            else {
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
                                &machine_component.allowed_module_categories,
                                &machine_component.allowed_effects,
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
            let module_slots = machine_component
                .module_slots
                .map(|slots| slots as usize)
                .unwrap_or(0);
            let combos = module_combinations(allowed_modules.len(), module_slots, quality_involved);
            for (comb, dup) in combos {
                for quality in 0..recipe_quality_range {
                    let mut modules = Vec::new();
                    for (module_id, module) in allowed_modules.iter().enumerate() {
                        for _ in 0..(comb.get(module_id).copied().unwrap_or(0) * dup) {
                            modules.push(module.clone());
                        }
                    }
                    // 机器品质：用户偏好命中用偏好品质，否则主品质
                    let base = Mechanic::Recipe(metatorio_core::RecipeMechanic {
                        recipe: IdWithQuality::new(record.name.clone(), quality_name(ctx, quality)),
                        machine: machine.clone(),
                        module_config: ModuleConfig::default(),
                        fuel: None,
                    });
                    push_with_beacons(out, base, modules, beacons);
                }
            }
            kept_any = true;
        }
        // 无满足条件/解锁的机器（全被表面/可达性过滤）：退化为评分最低的一台——
        // 配方出现即视为有对应组装机（mod 合理设计），不管替代数量设置与可达性。
        if !kept_any {
            if let Some(machine) = machines.last() {
                let base = Mechanic::Recipe(metatorio_core::RecipeMechanic {
                    recipe: IdWithQuality::new(record.name.clone(), quality_name(ctx, 0)),
                    machine: machine.clone(),
                    module_config: ModuleConfig::default(),
                    fuel: None,
                });
                push_with_beacons(out, base, Vec::new(), beacons);
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
            &major_quality,
            |drill_record| {
                drill_record
                    .component::<MiningDrillComponent>()
                    .is_some_and(|drill| drill.resource_categories.contains(&category))
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
            let machine_name = &machine.id;
            let Some(drill_record) = store.get(PrototypeGroup::Entity, machine_name) else {
                continue;
            };
            // 采矿机表面条件过滤
            if let Some(properties) = &properties {
                if let Some(entity) = drill_record.component::<EntityComponent>() {
                    if !crate::planet::surface_condition_satisfied(
                        store,
                        &entity.surface_conditions,
                        properties,
                    ) {
                        continue;
                    }
                }
            }
            // 采矿机可达性过滤：当前项目科技未解锁的机器不枚举。
            if let Some(accessibility) = &options.accessibility {
                if !accessibility.is_accessible(&metatorio_core::Accessible::Entity(
                    machine_name.clone(),
                )) {
                    continue;
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
                        machine: machine.clone(),
                        module_config: ModuleConfig::default(),
                        fuel: None,
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
    // 变质 / 种植 / 物品燃料 / 火箭发射
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
                let available = crate::planet::seed_available_on_planet(
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
    let quality_range = options.quality_limit + 1;
    for record in store.group(PrototypeGroup::Entity) {
        // 能量机器（发电机/锅炉/反应堆）可达性过滤：当前项目科技未解锁的机器不枚举。
        if let Some(accessibility) = &options.accessibility {
            if !accessibility.is_accessible(&metatorio_core::Accessible::Entity(
                record.name.clone(),
            )) {
                continue;
            }
        }
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
            // 只按原型自带模式枚举一个候选：热交换器等原型自带
            // output-to-separate-pipe（水→蒸汽）；HeatFluidInside 是旧版
            // 锅炉缺省，仅当原型没有 mode 字段时才可能是它。两种模式都枚举
            // 会让 heat-exchanger 多出一条 Heat→FluidHeat 抽象流，与温度互转/
            // 提热机制流线性相关 → 求解器奇异（已实测复现）。
            out.push(Mechanic::Boiler(metatorio_core::BoilerMechanic {
                boiler: IdWithQuality::new(record.name.clone(), major_quality.clone()),
                fluid: fluid.clone(),
                temperature: None,
                fuel: None,
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
    // 太阳能：所有太阳能板 × 蓄电器组合（按品质展开）。
    // 太阳能板按满日照功率排序、蓄电器按容量排序，限制候选数量避免组合爆炸。
    let mut panels: Vec<(String, f64)> = Vec::new();
    let mut accumulators: Vec<(String, f64)> = Vec::new();
    for record in store.group(PrototypeGroup::Entity) {
        // 太阳能板/蓄电器可达性过滤：当前项目科技未解锁的不枚举。
        let accessible = options
            .accessibility
            .as_ref()
            .is_none_or(|acc| acc.is_accessible(&metatorio_core::Accessible::Entity(record.name.clone())));
        if !accessible {
            continue;
        }
        if let Some(panel) = record.component::<SolarPanelComponent>() {
            panels.push((record.name.clone(), panel.production.amount));
        }
        if let Some(accumulator) = record.component::<AccumulatorComponent>() {
            let capacity = accumulator
                .energy_source
                .buffer_capacity
                .map(|energy| energy.amount)
                .unwrap_or(0.0);
            accumulators.push((record.name.clone(), capacity));
        }
    }
    // 每种设备最多取 alternative_count 个（按功率/容量降序）。
    let pick_top = |list: &mut Vec<(String, f64)>, count: usize| {
        list.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        list.truncate(count.max(1));
        list.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>()
    };
    let panel_names = pick_top(&mut panels, options.alternative_count.max(1));
    let accumulator_names = pick_top(&mut accumulators, options.alternative_count.max(1));
    for panel in &panel_names {
        for accumulator in &accumulator_names {
            for quality in 0..quality_range {
                out.push(Mechanic::Solar(metatorio_core::SolarMechanic {
                    solar_panel: IdWithQuality::new(panel.clone(), quality_name(ctx, quality)),
                    accumulator: IdWithQuality::new(
                        accumulator.clone(),
                        major_quality.clone(),
                    ),
                }));
            }
        }
    }
}

/// 判定一个候选机制在当前项目可达性下是否可用。
/// 只有配方机制按其解锁科技过滤；其它机制（采矿/发电/太阳能等）的
/// 机器可达性已在 `enumerate_*` 里按机器实体过滤。
pub fn mechanic_accessible(
    store: &PrototypeStore,
    accessible: &Accessibility,
    mechanic: &Mechanic,
) -> bool {
    let Mechanic::Recipe(mechanic) = mechanic else {
        return true;
    };
    recipe_unlocked(store, accessible, &mechanic.recipe.id)
}

/// 配方是否已解锁：`enabled`、任一 `unlock-recipe` 科技可达，**或** 某个
/// 以它为 `fixed_recipe` 的建筑可达（解锁建筑即解锁其固定配方，py 的
/// bioport→guano 等 hidden+enabled=false 配方属此类）。
pub fn recipe_unlocked(store: &PrototypeStore, accessible: &Accessibility, name: &str) -> bool {
    let Some(record) = store.get(PrototypeGroup::Recipe, name) else {
        return true;
    };
    let Some(recipe) = record.component::<RecipeComponent>() else {
        return true;
    };
    if recipe.enabled {
        return true;
    }
    // 固定配方建筑：解锁该建筑视为解锁此配方。
    if store.group(PrototypeGroup::Entity).any(|entity| {
        entity
            .component::<AssemblingMachineComponent>()
            .is_some_and(|machine| machine.fixed_recipe == name)
            && accessible.is_accessible(&Accessible::Entity(entity.name.clone()))
    }) {
        return true;
    }
    store
        .group(PrototypeGroup::Technology)
        .any(|tech_record| {
            let Some(tech) = tech_record.component::<TechnologyComponent>() else {
                return false;
            };
            if !accessible.is_accessible(&Accessible::Tech(tech_record.name.clone())) {
                return false;
            }
            tech.effects.iter().any(|effect| {
                matches!(effect, Modifier::UnlockRecipe(unlock) if unlock.recipe == name)
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::MechanicId;

    #[test]
    fn effective_recipe_categories_defaults_to_crafting() {
        let recipe = RecipeComponent {
            categories: None,
            ..Default::default()
        };
        assert_eq!(effective_recipe_categories(&recipe), vec!["crafting"]);
        let recipe = RecipeComponent {
            categories: Some(vec!["smelting".to_string()]),
            ..Default::default()
        };
        assert_eq!(effective_recipe_categories(&recipe), vec!["smelting"]);
    }

    #[test]
    fn used_candidates_filters_aux_and_subthreshold_flows() {
        // 索引 0 是零成本转换流的辅助变量（MechanicId(u64::MAX)），应被剔除；
        // 索引 1 用量大于阈值保留；索引 2 用量接近 0 剔除。
        let candidates = vec!["aux", "kept", "dropped"];
        let prim = vec![
            (ExpandedVarId { mechanic: MechanicId(u64::MAX), variant: 0 }, 5.0),
            (ExpandedVarId { mechanic: MechanicId(1), variant: 0 }, 2.0),
            (ExpandedVarId { mechanic: MechanicId(2), variant: 0 }, 1e-12),
        ];
        let prim_scale = vec![
            (ExpandedVarId { mechanic: MechanicId(u64::MAX), variant: 0 }, 1.0),
            (ExpandedVarId { mechanic: MechanicId(1), variant: 0 }, 1.0),
            (ExpandedVarId { mechanic: MechanicId(2), variant: 0 }, 1.0),
        ];
        let used = used_candidates(&candidates, prim, prim_scale);
        assert_eq!(used, vec!["kept"]);
    }

    #[test]
    fn recipe_unlocked_follows_enabled_and_unlock_tech() {
        let dump = serde_json::json!({
            "recipe": {
                "enabled-recipe": { "type": "recipe", "name": "enabled-recipe", "enabled": true },
                "locked-recipe": {
                    "type": "recipe", "name": "locked-recipe", "enabled": false,
                    "ingredients": [], "results": [], "energy_required": 1.0
                }
            },
            "technology": {
                "unlocker": {
                    "type": "technology", "name": "unlocker",
                    "prerequisites": [], "enabled": true,
                    "effects": [{ "type": "unlock-recipe", "recipe": "locked-recipe" }],
                    "unit": { "count": 1, "time": 1, "ingredients": [] }
                }
            }
        });
        let store = PrototypeStore::load(&dump).expect("dump 加载失败");
        let accessibility =
            metatorio_core::compute_accessibility(&store, &metatorio_core::AccessibilityOptions {
                all_accessible: true,
                ..Default::default()
            });
        assert!(recipe_unlocked(&store, &accessibility, "enabled-recipe"));
        assert!(recipe_unlocked(&store, &accessibility, "locked-recipe"));
    }
}
