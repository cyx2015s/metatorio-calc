use crate::{
    concept::SolveContext,
    factorio::{
        common::*,
        editor::{icon::Icon, modal::show_modal},
        model::{context::*, entity::*},
    },
};

#[derive(Debug, Clone, serde::Deserialize)]
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

#[derive(Debug, Clone, serde::Deserialize)]
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

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BeaconCounter {
    #[default]
    Total,
    SameType,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ModuleConfig {
    pub modules: Vec<IdWithQuality>,
    pub beacons: Vec<BeaconConfig>,
    pub extra_effects: Effect,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
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

fn module_effects_allowed(
    module: &ModulePrototype,
    allowed_effects: &Option<EffectTypeLimitation>,
) -> bool {
    if let Some(allowed_effects) = allowed_effects {
        if let EffectTypeLimitation::Multiple(normalized) = allowed_effects.normalized() {
            (normalized.contains(&EffectType::Consumption) || module.effect.consumption >= 0.0) //  要么允许节能，要么模块本身不减少能耗
                && (normalized.contains(&EffectType::Speed) || module.effect.speed <= 0.0) // 要么允许加速，要么模块本身不增加速度
                && (normalized.contains(&EffectType::Productivity)
                    || module.effect.productivity <= 0.0) // 要么允许产能，要么模块本身不增加产能
                && (normalized.contains(&EffectType::Pollution) || module.effect.pollution <= 0.0) // 要么允许污染，要么模块本身不减少污染
                && (normalized.contains(&EffectType::Quality) || module.effect.quality <= 0.0) // 要么允许品质，要么模块本身不增加品质
        } else {
            unreachable!();
        }
    } else {
        module.effect.productivity <= 0.0
    }
}

impl egui::Widget for ModuleConfigEditor<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let button = ui
            .add_sized(
                [35.0, 35.0],
                Icon {
                    ctx: self.ctx,
                    type_name: "item",
                    item_name: "empty-module-slot",
                    size: 35.0,
                    quality: 0,
                },
            )
            .interact(egui::Sense::click());
        show_modal(button.id, button.clicked(), ui, |ui| {
            
        });
        ui.response().clone()
    }
}
