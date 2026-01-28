use indexmap::IndexMap;

use crate::{
    concept::SolveContext,
    factorio::{
        common::*,
        editor::{
            icon::{GenericIcon, Icon},
            modal::show_modal,
        },
        format::CompactLabel,
        model::{context::*, entity::*},
        selector::item_with_quality_selector_modal,
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

fn floor_to_percentage(value: f64) -> f64 {
    (value * 100.0).trunc() / 100.0
}

pub fn effects_under_quality(effect: &Effect, multiplier: f64) -> Effect {
    let mut effect = effect.clone();
    if effect.consumption < 0.0 {
        effect.consumption *= multiplier;
        effect.consumption = floor_to_percentage(effect.consumption);
    }
    if effect.speed > 0.0 {
        effect.speed *= multiplier;
        effect.speed = floor_to_percentage(effect.speed);
    }
    if effect.productivity > 0.0 {
        effect.productivity *= multiplier;
        effect.productivity = floor_to_percentage(effect.productivity);
    }
    if effect.pollution < 0.0 {
        effect.pollution *= multiplier;
        effect.pollution = floor_to_percentage(effect.pollution);
    }
    if effect.quality > 0.0 {
        effect.quality *= multiplier;
        effect.quality = floor_to_percentage(effect.quality);
    }
    effect
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
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct BeaconConfig {
    pub modules: IndexMap<IdWithQuality, usize>, // 这种插件塔中，这个插件有多少个，不是每个插件塔中的插件！
    pub beacon: IdWithQuality,                   // 插件塔本身
    pub count: usize,                            // 插件塔的数量
}

impl ModuleConfig {
    pub fn new() -> Self {
        Self {
            modules: vec![],
            beacons: vec![],
        }
    }

    pub fn get_effect(&self, ctx: &FactorioContext) -> Effect {
        let mut total_effect = Effect::default();
        for module in &self.modules {
            if let Some(module_proto) = ctx.modules.get(&module.0) {
                total_effect = total_effect
                    + effects_under_quality(
                        &module_proto.effect,
                        ctx.qualities[module.1 as usize].default_multiplier(),
                    );
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
            (normalized.contains(&EffectType::Consumption) || module.effect.consumption >= 0.0) //  要么允许节能，要么插件本身不减少能耗
                && (normalized.contains(&EffectType::Speed) || module.effect.speed <= 0.0) // 要么允许加速，要么插件本身不增加速度
                && (normalized.contains(&EffectType::Productivity)
                    || module.effect.productivity <= 0.0) // 要么允许产能，要么插件本身不增加产能
                && (normalized.contains(&EffectType::Pollution) || module.effect.pollution <= 0.0) // 要么允许污染，要么插件本身不减少污染
                && (normalized.contains(&EffectType::Quality) || module.effect.quality <= 0.0) // 要么允许品质，要么插件本身不增加品质
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
            .vertical(|ui| {
                ui.label("插件");
                ui.button("编辑")
            })
            .inner;
        ui.horizontal(|ui| {
            // 获取所有插件和信标的综合
            let mut total = IndexMap::new();
            for module in &self.module_config.modules {
                index_map_update_entry(
                    &mut total,
                    GenericItem::Item {
                        name: module.0.clone(),
                        quality: module.1,
                    },
                    1,
                );
            }
            for beacon_config in &self.module_config.beacons {
                for (module, count) in &beacon_config.modules {
                    index_map_update_entry(
                        &mut total,
                        GenericItem::Item {
                            name: module.0.clone(),
                            quality: module.1,
                        },
                        *count,
                    );
                }
                index_map_update_entry(
                    &mut total,
                    GenericItem::Entity {
                        name: beacon_config.beacon.0.clone(),
                        quality: beacon_config.beacon.1,
                    },
                    beacon_config.count,
                );
            }
            for (item, count) in total {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = [3.0, 3.0].into();
                    ui.add_sized(
                        [32.0, 32.0],
                        GenericIcon {
                            ctx: self.ctx,
                            item: &item,
                            size: 32.0,
                        },
                    );
                    ui.add_sized([35.0, 15.0], CompactLabel::new(count as f64));
                });
            }
        });
        show_modal(button.id, button.clicked(), ui, |ui| {
            ui.label("编辑插件");
            let len = self.module_config.modules.len();
            ui.horizontal(|ui| {
                self.module_config.modules.retain_mut(|slot| {
                    let mut deleted = false;
                    let icon = ui
                        .add_sized(
                            [35.0, 35.0],
                            Icon {
                                ctx: self.ctx,
                                type_name: "item",
                                item_name: &slot.0,
                                quality: slot.1,
                                size: 32.0,
                            },
                        )
                        .interact(egui::Sense::click());
                    if icon.clicked_by(egui::PointerButton::Secondary) {
                        deleted = true;
                    }
                    let selected = item_with_quality_selector_modal(
                        ui,
                        self.ctx,
                        "选择插件",
                        "item",
                        &icon,
                        Some(&|s, f| {
                            if let Some(module_proto) = f.modules.get(s) {
                                // 过滤掉不符合要求的插件
                                self.allowed_module_categories.as_ref().map_or(
                                    true,
                                    |allowed_categories| {
                                        allowed_categories.contains(&module_proto.category)
                                    },
                                ) && module_effects_allowed(module_proto, self.allowed_effects)
                            } else {
                                false
                            }
                        }),
                    );
                    if let Some(selected) = selected {
                        slot.0 = selected.0;
                        slot.1 = selected.1;
                    }
                    !deleted
                });

                for idx in len..self.module_slots {
                    let icon = ui
                        .add_sized(
                            [35.0, 35.0],
                            Icon {
                                ctx: self.ctx,
                                type_name: "item",
                                item_name: "empty-module-slot",
                                quality: 0,
                                size: 32.0,
                            },
                        )
                        .interact(egui::Sense::click());
                    let selected = item_with_quality_selector_modal(
                        ui,
                        self.ctx,
                        "选择插件",
                        "item",
                        &icon,
                        Some(&|s, f| {
                            if let Some(module_proto) = f.modules.get(s) {
                                // 过滤掉不符合要求的插件
                                self.allowed_module_categories.as_ref().map_or(
                                    true,
                                    |allowed_categories| {
                                        allowed_categories.contains(&module_proto.category)
                                    },
                                ) && module_effects_allowed(module_proto, self.allowed_effects)
                            } else {
                                false
                            }
                        }),
                    );
                    if let Some(selected) = selected {
                        while self.module_config.modules.len() <= idx {
                            self.module_config.modules.push(selected.clone());
                        }
                    }
                }
            });
        });
        ui.response().clone()
    }
}
