use egui::Grid;
use serde::Deserialize;

use crate::{
    concept::SolveContext,
    factorio::{
        common::*,
        model::{context::*, entity::*},
    },
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

impl SolveContext for ModuleConfig {
    type GameContext = FactorioContext;
    type ItemIdentType = GenericItem;
}

pub struct ModuleConfigEditor<'a> {
    pub module_config: &'a mut ModuleConfig,

    pub module_slots: usize,
    pub allowed_effects: &'a Option<EffectTypeLimitation>,
    pub allowed_module_categories: &'a Option<Vec<String>>,

    pub ctx: &'a FactorioContext,
}

impl<'a> ModuleConfigEditor<'a> {
    pub fn new<'b>(
        ctx: &'b FactorioContext,
        module_config: &'b mut ModuleConfig,
        module_slots: usize,
        allowed_effects: &'b Option<EffectTypeLimitation>,
        allowed_module_categories: &'b Option<Vec<String>>,
    ) -> Self
    where
        'b: 'a,
    {
        Self {
            module_config,
            module_slots,
            allowed_effects,
            allowed_module_categories,
            ctx,
        }
    }
}

impl egui::Widget for ModuleConfigEditor<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        Grid::new("插件加成").num_columns(2).show(ui, |ui| {
            // ui.label(form)
        });
        ui.response().clone()
    }
}
