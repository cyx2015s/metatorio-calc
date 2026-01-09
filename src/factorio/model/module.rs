use serde::Deserialize;

use crate::factorio::{
    common::{
        Effect, EffectTypeLimitation, EnergyAmount, EnergySource, IdWithQuality, PrototypeBase,
        option_as_vec_or_empty,
    },
    model::{context::FactorioContext, entity::EntityPrototype},
};

#[derive(Debug, Clone, Deserialize)]
pub struct ModulePrototype {
    #[serde(flatten)]
    pub base: PrototypeBase,

    /// 增强效果
    pub effect: Effect,

    /// 可安装的机器类别
    pub category: String,

    /// 等级
    pub tier: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BeaconPrototype {
    #[serde(flatten)]
    pub base: EntityPrototype,

    pub energy_usage: EnergyAmount,
    pub energy_source: EnergySource,

    #[serde(default)]
    pub distribution_effectivity: f64,
    #[serde(default)]
    pub distribution_effectivity_bonus_per_quality_level: f64,
    pub module_slots: f64,
    #[serde(default)]
    pub quality_affects_module_slots: bool,
    #[serde(default)]
    pub allowed_effects: Option<EffectTypeLimitation>,

    #[serde(deserialize_with = "option_as_vec_or_empty")]
    #[serde(default)]
    pub allowed_module_categories: Option<Vec<String>>,

    #[serde(default)]
    pub beacon_counter: BeaconCounter,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BeaconCounter {
    #[default]
    Total,
    SameType,
}

#[derive(Debug, Clone, Default)]
pub struct ModuleConfig {
    pub modules: Vec<IdWithQuality>,
    pub beacons: Vec<BeaconConfig>,
    pub extra_effects: Effect,
}

#[derive(Debug, Clone, Default)]
pub struct BeaconConfig {
    pub modules: Vec<IdWithQuality>,
    pub count: usize,
}

impl ModuleConfig {
    pub fn new() -> Self {
        Self {
            modules: vec![],
            beacons: vec![],
            extra_effects: Effect::default(),
        }
    }

    pub fn get_effect(&self, ctx: &FactorioContext) -> Effect {
        let mut total_effect = self.extra_effects.clone();
        for module in &self.modules {
            if let Some(module_proto) = ctx.modules.get(&module.0) {
                total_effect = total_effect + module_proto.effect.clone();
            }
        }
        total_effect
    }
}
