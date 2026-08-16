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
use metatorio_data::generated_components::{BeaconComponent, ModuleComponent, QualityComponent};
use metatorio_data::types::{BeaconCounter, Effect, EnergySource};

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
    let name = ctx.prototype.quality_order().get(level)?;
    ctx.prototype
        .get(metatorio_data::store::PrototypeGroup::Quality, name)
        .and_then(|record| record.component::<QualityComponent>())
}

pub fn module_effect_at_quality(module: &ModuleComponent, quality: &QualityComponent) -> Effect {
    let raw_effect = module.effect;
    let scale = |value: f64, module_factor: f64, quality_factor: f64| {
        value * (1.0 - module_factor + quality_factor * module_factor)
    };
    Effect {
        consumption: scale(
            raw_effect.consumption,
            module.consumption_quality_multiplier(),
            quality.module_consumption_multiplier(),
        ),
        pollution: scale(
            raw_effect.pollution,
            module.pollution_quality_multiplier(),
            quality.module_pollution_multiplier(),
        ),
        productivity: scale(
            raw_effect.productivity,
            module.productivity_quality_multiplier(),
            quality.module_productivity_multiplier(),
        ),
        quality: scale(
            raw_effect.quality,
            module.quality_quality_multiplier(),
            quality.module_quality_multiplier(),
        ),
        speed: scale(
            raw_effect.speed,
            module.speed_quality_multiplier(),
            quality.module_speed_multiplier(),
        ),
    }
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
                let effect = quality_by_level(ctx, quality)
                    .map(|q| module_effect_at_quality(module_proto, q))
                    .unwrap_or(module_proto.effect);
                total_effect = total_effect + effect;
            }
        }
        let mut beacon_count = 0usize;
        let mut beacon_count_by_type: crate::prim_var::AIndexMap<String, usize> =
            Default::default();
        for bc in &self.beacons {
            if beacon_prototype(ctx, &bc.beacon.id)
                .and_then(|record| record.component::<BeaconComponent>())
                .is_some()
            {
                beacon_count += bc.count;
                *beacon_count_by_type
                    .entry(bc.beacon.id.clone())
                    .or_insert(0) += bc.count;
            }
        }
        for bc in &self.beacons {
            if bc.count == 0 || beacon_count == 0 {
                continue;
            }
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
                        let module_effect = quality_by_level(ctx, module_quality)
                            .map(|q| module_effect_at_quality(module_proto, q))
                            .unwrap_or(module_proto.effect);
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
                let energy_usage = match &bp.energy_source {
                    EnergySource::Electric(_) => bp.energy_usage.amount * 60.0,
                    EnergySource::Void => 0.0,
                    _ => 0.0,
                };
                let quality = ctx.game.quality_level(&bc.beacon.quality);
                let quality_multiplier = quality_by_level(ctx, quality)
                    .map(|q| q.beacon_power_usage_multiplier)
                    .unwrap_or(1.0);
                let consumption_per_beacon = energy_usage / bc.share.max(1.0);
                total_consumption += consumption_per_beacon * bc.count as f64 * quality_multiplier;
            }
        }
        total_consumption
    }
}

/// 单个信标（插件塔）的配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// "添加信标"推入的默认配置：1 座信标、覆盖 1 台机器、normal 品质。
/// count = 0 会让信标行完全无效（无加成/无耗电），share = 0 会被
/// `max(1.0)` 静默掩盖成 1——两个默认值都不该是 0。
impl Default for BeaconConfig {
    fn default() -> Self {
        Self {
            modules: Vec::new(),
            beacon: IdWithQuality::default(),
            count: 1,
            share: 1.0,
        }
    }
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
    /// 明确燃料 ID。流体能量源使用流体，Burner 能量源使用物品；
    /// Electric/Heat/Void 时无效（None）。
    pub fuel: Option<String>,
    /// 指定流体燃料温度；None 使用该流体的默认温度。
    pub fuel_temperature: Option<i32>,
}

/// 采矿组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MiningMechanic {
    pub resource: String,
    pub machine: IdWithQuality,
    pub module_config: ModuleConfig,
    /// 明确燃料 ID；None = 自动选择/无需燃料。
    pub fuel: Option<String>,
    /// 指定流体燃料温度；None 使用该流体的默认温度。
    pub fuel_temperature: Option<i32>,
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
    /// 输入流体温度；None 使用流体默认温度。
    pub temperature: Option<i32>,
}

/// 锅炉组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoilerMechanic {
    pub boiler: IdWithQuality,
    pub fluid: String,
    /// 输入流体温度；None 使用流体默认温度。
    pub temperature: Option<i32>,
    /// 明确燃料 ID；None = 自动选择/无需燃料。
    pub fuel: Option<String>,
    /// 指定流体燃料温度；None 使用该流体的默认温度。
    pub fuel_temperature: Option<i32>,
}

/// 反应堆组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReactorMechanic {
    pub reactor: IdWithQuality,
    pub neighbours: u8,
    /// 明确燃料 ID（反应堆通常使用物品）；None = 自动选择/无需燃料。
    pub fuel: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{Context, GameState};
    use metatorio_data::store::PrototypeStore;

    #[test]
    fn beacon_modules_contribute_to_effect() {
        let dump = serde_json::json!({
            "module": {
                "speed-module-3": {
                    "name": "speed-module-3",
                    "category": "speed",
                    "tier": 3,
                    "effect": { "speed": 0.5, "consumption": 0.5, "productivity": 0.0, "pollution": 0.0, "quality": 0.0 }
                }
            },
            "beacon": {
                "beacon": {
                    "name": "beacon",
                    "distribution_effectivity": 1.0,
                    "module_slots": 2,
                    "energy_usage": "480kW",
                    "energy_source": { "type": "electric" }
                }
            },
            "quality": {
                "normal": { "name": "normal", "level": 0 }
            }
        });
        let store = PrototypeStore::load(&dump).expect("dump 加载失败");
        let game = GameState {
            qualities: vec!["normal".to_string()],
            max_quality: 0,
            ..Default::default()
        };
        let ctx = Context::new(&store, &game);

        // 无信标：速度为 0。
        let without = ModuleConfig {
            modules: vec![],
            beacons: vec![],
        };
        assert_eq!(without.get_effect(&ctx).speed, 0.0);
        // 一个信标 + 2 个速度插件 → 速度 > 0。
        let with_beacon = ModuleConfig {
            modules: vec![],
            beacons: vec![BeaconConfig {
                beacon: IdWithQuality::new("beacon", "normal"),
                count: 1,
                share: 1.0,
                modules: vec![(IdWithQuality::new("speed-module-3", "normal"), 2)],
            }],
        };
        let effect = with_beacon.get_effect(&ctx);
        assert!(
            effect.speed > 0.0,
            "信标中的插件应计入产出加成：{effect:?}"
        );
        // 信标耗电（均摊）> 0。
        assert!(with_beacon.get_consumption(&ctx) > 0.0);
    }
}
