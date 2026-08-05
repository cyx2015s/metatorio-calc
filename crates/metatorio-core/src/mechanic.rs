//! 工厂组件（Mechanic）枚举与模块配置。
//!
//! **`Mechanic` 表示工厂的 1 个组件（单个生产单元）**，不是列表：
//! 如 `Mechanic::Recipe(RecipeMechanic)` 即 1 个配方 + 1 个机器 + 可选燃料 +
//! 可选插件配置；工厂整体是用户层的 `Vec<Mechanic>`。
//!
//! 纯数据层：UI 状态（suggestion_*）、求解逻辑（AsFlow）与偏好配置
//! （machine_preferences/enumerate_*）均不在此层。

use serde::{Deserialize, Serialize};

use crate::id::IdWithQuality;

// ── 模块配置（ModuleConfig 体系，纯数据）─────────────────────────

/// 一个机器实例的模块/信标配置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModuleConfig {
    pub modules: Vec<IdWithQuality>,
    pub beacons: Vec<BeaconConfig>,
}

// 模块/插件塔效果与耗电（迁移自 metatorio-egui ModuleConfig::get_effect/get_consumption）

use crate::context::Context;
use metatorio_data::generated_components::{BeaconComponent, ModuleComponent};
use metatorio_data::types::{BeaconCounter, Effect};

fn module_prototype<'a>(
    ctx: &'a Context,
    name: &str,
) -> Option<&'a metatorio_data::store::PrototypeRecord> {
    // module 原型含 ItemComponent → Item 组
    ctx.prototype.item(name)
}

fn beacon_prototype<'a>(
    ctx: &'a Context,
    name: &str,
) -> Option<&'a metatorio_data::store::PrototypeRecord> {
    // beacon 原型含 EntityComponent → Entity 组
    ctx.prototype.entity(name)
}

/// 品质组件按 order 排序后的第 `level` 个。
pub(crate) fn quality_by_level<'a>(
    ctx: &'a Context,
    level: usize,
) -> Option<&'a metatorio_data::generated_components::QualityComponent> {
    crate::quality::sorted_qualities(ctx).into_iter().nth(level)
}

pub fn module_effects_under_quality(module: &ModuleComponent, level: u32) -> Effect {
    let raw_effect = module.effect;
    let mul = level as f64;
    Effect {
        consumption: raw_effect.consumption * (1.0 + module.consumption_quality_multiplier() * mul),
        pollution: raw_effect.pollution * (1.0 + module.pollution_quality_multiplier() * mul),
        productivity: raw_effect.productivity
            * (1.0 + module.productivity_quality_multiplier() * mul),
        quality: raw_effect.quality + (1.0 + module.quality_quality_multiplier() * mul),
        speed: raw_effect.speed * (1.0 + module.speed_quality_multiplier() * mul),
    }
}

/// 效果按品质缩放：负向效果（consumption/pollution）与正向效果（speed/productivity/quality）
/// 分别乘品质倍率（迁移自 egui effects_under_quality）。
pub fn effects_under_quality(effect: &Effect, multiplier: f64) -> Effect {
    let mut effect = *effect;
    if effect.consumption < 0.0 {
        effect.consumption *= multiplier;
    }
    if effect.speed > 0.0 {
        effect.speed *= multiplier;
    }
    if effect.productivity > 0.0 {
        effect.productivity *= multiplier;
    }
    if effect.pollution < 0.0 {
        effect.pollution *= multiplier;
    }
    if effect.quality > 0.0 {
        effect.quality *= multiplier;
    }
    effect
}

impl ModuleConfig {
    /// 模块与插件塔的效果汇总（迁移自 egui ModuleConfig::get_effect）。
    pub fn get_effect(&self, ctx: &Context) -> Effect {
        let mut total_effect = Effect::default();
        for module in &self.modules {
            if let Some(record) = module_prototype(ctx, &module.id)
                && let Some(module_proto) = record.component::<ModuleComponent>()
            {
                let quality = ctx.game.quality_level(&module.quality);
                let multiplier = quality_by_level(ctx, quality)
                    .map(|q| q.default_multiplier())
                    .unwrap_or(1.0);
                total_effect =
                    total_effect + effects_under_quality(&module_proto.effect, multiplier);
            }
        }
        let mut beacon_count = 0usize;
        let mut beacon_count_by_type: crate::prim_var::AIndexMap<String, usize> =
            Default::default();
        for bc in &self.beacons {
            beacon_count += bc.count;
            *beacon_count_by_type
                .entry(bc.beacon.id.clone())
                .or_insert(0) += bc.count;
        }
        for bc in &self.beacons {
            if let Some(record) = beacon_prototype(ctx, &bc.beacon.id)
                && let Some(bp) = record.component::<BeaconComponent>()
            {
                let beacon_quality = ctx.game.quality_level(&bc.beacon.quality);
                let effective_module_slots = if bp.quality_affects_module_slots {
                    let bonus = quality_by_level(ctx, beacon_quality)
                        .map(|q| q.beacon_module_slots_bonus())
                        .unwrap_or(0);
                    bp.module_slots as usize + bonus as usize
                } else {
                    bp.module_slots as usize
                };
                let profile_multiplier = match bp.beacon_counter {
                    Some(BeaconCounter::SameType) => {
                        let count = beacon_count_by_type
                            .get(&bc.beacon.id)
                            .copied()
                            .unwrap_or(0);
                        bp.get_profile(count)
                    }
                    _ => bp.get_profile(beacon_count),
                };
                let base_efficiency = bp.distribution_effectivity
                    + bp.distribution_effectivity_bonus_per_quality_level
                        .unwrap_or(0.0)
                        * quality_by_level(ctx, beacon_quality)
                            .map(|q| q.level as f64)
                            .unwrap_or(0.0);
                for (module, count) in &bc.modules {
                    if let Some(record) = module_prototype(ctx, &module.id)
                        && let Some(module_proto) = record.component::<ModuleComponent>()
                    {
                        let module_quality = ctx.game.quality_level(&module.quality);
                        let multiplier = quality_by_level(ctx, module_quality)
                            .map(|q| q.default_multiplier())
                            .unwrap_or(1.0);
                        let module_effect = effects_under_quality(&module_proto.effect, multiplier);
                        let count = (*count).min(effective_module_slots * beacon_count);
                        let total_module_effect =
                            module_effect * count as f64 * base_efficiency * profile_multiplier;
                        total_effect = total_effect + total_module_effect;
                    }
                }
            }
        }
        total_effect
    }

    /// 插件塔耗电量（考虑共享比例均摊），单位 W（每秒）。
    pub fn get_consumption(&self, ctx: &Context) -> f64 {
        let mut total_consumption = 0.0;
        for bc in &self.beacons {
            if let Some(record) = beacon_prototype(ctx, &bc.beacon.id)
                && let Some(bp) = record.component::<BeaconComponent>()
            {
                let energy_usage = bp.energy_usage.amount * 60.0; // 每 tick → 每秒
                let consumption_per_beacon = energy_usage / bc.share.max(1.0);
                total_consumption += consumption_per_beacon * bc.count as f64;
            }
        }
        total_consumption
    }
}

/// 单个信标（插件塔）的配置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BeaconConfig {
    /// 这种插件塔中的模块（数量是塔内模块数，不是塔数量）。
    pub modules: Vec<(IdWithQuality, usize)>,
    /// 插件塔本身。
    pub beacon: IdWithQuality,
    /// 插件塔的数量。
    pub count: usize,
    /// 插件塔共享比例：值为 x 表示平均一个插件塔能覆盖到 x 个机器，
    /// 计算插件塔的耗电时需要除以相应的数量。
    pub share: f64,
}

// ── Mechanic 枚举 ────────────────────────────────────────────────
// 工厂的一个组件（单例配置）。每个变体持有 1 个 struct（不内联）。
// #[non_exhaustive]：机制集合会继续扩展，禁止外部穷尽匹配。

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Mechanic {
    Recipe(RecipeMechanic),
    Mining(MiningMechanic),
    Spoil(SpoilMechanic),
    Plant(PlantMechanic),
    ItemFuel(ItemFuelMechanic),
    ItemLaunch(ItemLaunchMechanic),
    Generator(GeneratorMechanic),
    Boiler(BoilerMechanic),
    Reactor(ReactorMechanic),
}

// ── 组件配置 struct（迁移自 metatorio-egui 的 XxxInstance，单例语义）──

/// 配方组件：在机器中按模块配置生产指定配方。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecipeMechanic {
    pub recipe: IdWithQuality,
    pub machine: IdWithQuality,
    pub module_config: ModuleConfig,
    /// 流体燃料名（FluidID）。只有流体供能的机器会因温度低/热值低导致速率跑不满，
    /// 需要用户指定燃料流体；物品燃料无此问题（求解时自动选择最优），
    /// Electric/Heat/Void 时无效（None）。
    pub fuel: Option<String>,
}

/// 采矿组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MiningMechanic {
    pub resource: String,
    pub machine: IdWithQuality,
    pub module_config: ModuleConfig,
    /// 流体燃料名（FluidID）；None = 自动选择/无需燃料。
    pub fuel: Option<String>,
}

/// 腐坏组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpoilMechanic {
    pub item: IdWithQuality,
}

/// 种植组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlantMechanic {
    pub seed: IdWithQuality,
}

/// 物品燃料组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemFuelMechanic {
    pub item: IdWithQuality,
}

/// 物品发射（火箭运力）组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemLaunchMechanic {
    pub item: IdWithQuality,
    /// true = 重量火箭（RocketWeightCapacity），false = 堆叠火箭（RocketSlotCapacity）。
    pub weight_mode: bool,
}

/// 发电机组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneratorMechanic {
    pub generator: IdWithQuality,
    pub fluid: String,
}

/// 锅炉组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoilerMechanic {
    pub boiler: IdWithQuality,
    pub fluid: String,
    /// 流体燃料名（FluidID）；None = 自动选择/无需燃料。
    pub fuel: Option<String>,
}

/// 反应堆组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReactorMechanic {
    pub reactor: IdWithQuality,
    pub neighbours: u8,
    /// 流体燃料名（FluidID）；None = 自动选择/无需燃料。
    pub fuel: Option<String>,
}
