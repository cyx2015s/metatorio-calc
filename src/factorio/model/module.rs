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
        modal::ItemWithQualitySelectorModal,
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

    #[serde(default, deserialize_with = "option_as_vec_or_empty")]
    pub profile: Option<Vec<f64>>,
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

#[derive(Debug, Clone, serde::Deserialize, Default, PartialEq, Eq)]
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
    pub modules: Vec<(IdWithQuality, usize)>, // 这种插件塔中，这个插件有多少个，不是每个插件塔中的插件！
    pub beacon: IdWithQuality,                // 插件塔本身
    pub count: usize,                         // 插件塔的数量
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
        let mut beacon_count = 0;
        for beacon_config in &self.beacons {
            if ctx.beacons.contains_key(&beacon_config.beacon.0) {
                beacon_count += beacon_config.count;
            }
        }
        for beacon_config in &self.beacons {
            if let Some(beacon_proto) = ctx.beacons.get(&beacon_config.beacon.0) {
                let effective_module_slots = if beacon_proto.quality_affects_module_slots {
                    let quality_bonus =
                        ctx.qualities[beacon_config.beacon.1 as usize].beacon_module_slots_bonus();
                    beacon_proto.module_slots as usize + quality_bonus as usize
                } else {
                    beacon_proto.module_slots as usize
                };
                let profile_multiplier = match beacon_proto.beacon_counter {
                    BeaconCounter::SameType => {
                        let profile = &beacon_proto.profile;
                        match profile {
                            Some(profile) => {
                                if beacon_config.count - 1 < profile.len() {
                                    profile[beacon_config.count - 1]
                                } else {
                                    *profile.last().unwrap_or(&1.0)
                                }
                            }
                            None => 1.0,
                        }
                    }
                    BeaconCounter::Total => {
                        if let Some(profile) = &beacon_proto.profile {
                            let idx = beacon_count - 1;
                            if idx < profile.len() {
                                profile[idx]
                            } else {
                                *profile.last().unwrap_or(&1.0)
                            }
                        } else {
                            1.0
                        }
                    }
                };
                let base_efficiency = beacon_proto.distribution_effectivity
                    + beacon_proto.distribution_effectivity_bonus_per_quality_level
                        * ctx.qualities[beacon_config.beacon.1 as usize].level;
                for (module, count) in &beacon_config.modules {
                    if let Some(module_proto) = ctx.modules.get(&module.0) {
                        let module_effect = effects_under_quality(
                            &module_proto.effect,
                            ctx.qualities[module.1 as usize].default_multiplier(),
                        );
                        let total_module_effect = module_effect
                            * (*count as f64)
                            * (effective_module_slots as f64)
                            * base_efficiency
                            * profile_multiplier;
                        total_effect = total_effect + total_module_effect;
                    }
                }
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
    pub changed: Option<&'a mut bool>,
}

impl<'a> ModuleConfigEditor<'a> {
    pub fn new(
        ctx: &'a FactorioContext,
        module_config: &'a mut ModuleConfig,
        module_slots: usize,
        allowed_effects: &'a Option<EffectTypeLimitation>,
        allowed_module_categories: &'a Option<Vec<String>>,
    ) -> Self {
        Self {
            module_config,
            module_slots,
            allowed_effects,
            allowed_module_categories,
            ctx,
            changed: None,
        }
    }

    pub fn notify_change(mut self, changed: &'a mut bool) -> Self {
        self.changed = Some(changed);
        self
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
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        let button = ui
            .vertical(|ui| {
                ui.label("插件");
                if self.module_slots == 0 {
                    ui.disable();
                }
                ui.button("编辑")
            })
            .inner;
        if self.module_slots == 0 {
            return ui.response().clone();
        }
        ui.horizontal(|ui| {
            // 获取所有插件和信标的综合
            let mut total = IndexMap::new();
            for module in &self.module_config.modules {
                index_map_update_entry(
                    &mut total,
                    GenericItem::Item(IdWithQuality(module.0.clone(), module.1)),
                    1,
                );
            }
            for beacon_config in &self.module_config.beacons {
                for (module, count) in &beacon_config.modules {
                    index_map_update_entry(
                        &mut total,
                        GenericItem::Item(IdWithQuality(module.0.clone(), module.1)),
                        *count,
                    );
                }
                index_map_update_entry(
                    &mut total,
                    GenericItem::Entity(IdWithQuality(
                        beacon_config.beacon.0.clone(),
                        beacon_config.beacon.1,
                    )),
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
            ui.vertical(|ui| {
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
                        let mut widget = ItemWithQualitySelectorModal::new(
                            icon.id,
                            self.ctx,
                            "选择插件",
                            "item",
                        )
                        .with_toggle(icon.clicked())
                        .with_current(slot)
                        .with_filter(|s, f| {
                            if let Some(module_proto) = f.modules.get(s) {
                                // 过滤掉不符合要求的插件
                                self.allowed_module_categories.as_ref().is_none_or(
                                    |allowed_categories| {
                                        allowed_categories.contains(&module_proto.category)
                                    },
                                ) && module_effects_allowed(module_proto, self.allowed_effects)
                            } else {
                                false
                            }
                        });
                        if let Some(changed) = &mut self.changed {
                            widget = widget.notify_change(changed);
                        }
                        ui.add(widget);
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
                        let mut selected = None;
                        let mut widget = ItemWithQualitySelectorModal::new(
                            icon.id,
                            self.ctx,
                            "填充插件",
                            "item",
                        )
                        .with_toggle(icon.clicked())
                        .with_output(&mut selected)
                        .with_filter(|s, f| {
                            if let Some(module_proto) = f.modules.get(s) {
                                // 过滤掉不符合要求的插件
                                self.allowed_module_categories.as_ref().is_none_or(
                                    |allowed_categories| {
                                        allowed_categories.contains(&module_proto.category)
                                    },
                                ) && module_effects_allowed(module_proto, self.allowed_effects)
                            } else {
                                false
                            }
                        });
                        if let Some(changed) = &mut self.changed {
                            widget = widget.notify_change(changed);
                        }
                        ui.add(widget);
                        if let Some(selected) = selected {
                            while self.module_config.modules.len() <= idx {
                                self.module_config.modules.push(selected.clone());
                            }
                        }
                    }
                });
                ui.separator();
                ui.label("插件塔");
                self.module_config.beacons.retain_mut(|beacon_config| {
                    let mut deleted = false;
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            if ui.button("删除").clicked() {
                                deleted = true;
                            }
                            let widget = egui::DragValue::new(&mut beacon_config.count)
                                .range(1..=100)
                                .clamp_existing_to_range(true);
                            if ui.add(widget).changed() {
                                if let Some(changed) = &mut self.changed {
                                    **changed = true;
                                }
                            }
                        });
                        ui.separator();
                        ui.vertical(|ui| {
                            let icon = if let Some(beacon_proto) =
                                self.ctx.beacons.get(&beacon_config.beacon.0)
                            {
                                ui.add_sized(
                                    [35.0, 35.0],
                                    Icon {
                                        ctx: self.ctx,
                                        type_name: "entity",
                                        item_name: &beacon_config.beacon.0,
                                        size: 32.0,
                                        quality: beacon_config.beacon.1,
                                    },
                                )
                                .on_hover_text(
                                    self.ctx.get_display_name("entity", &beacon_config.beacon.0),
                                )
                            } else {
                                ui.add_sized(
                                    [35.0, 35.0],
                                    Icon {
                                        ctx: self.ctx,
                                        type_name: "entity",
                                        item_name: "entity-unknown",
                                        size: 32.0,
                                        quality: 0,
                                    },
                                )
                                .on_hover_text("未选择插件塔")
                            }
                            .interact(egui::Sense::click());
                            let mut widget = ItemWithQualitySelectorModal::new(
                                icon.id,
                                self.ctx,
                                "选择插件塔",
                                "entity",
                            )
                            .with_toggle(icon.clicked())
                            .with_current(&mut beacon_config.beacon)
                            .with_filter(|s, f| f.beacons.contains_key(s));
                            if let Some(changed) = &mut self.changed {
                                widget = widget.notify_change(changed);
                            }
                            ui.add(widget);
                        });
                        ui.separator();
                        if let Some(beacon_proto) = self.ctx.beacons.get(&beacon_config.beacon.0) {
                            let mut total_modules = 0;
                            beacon_config.modules.retain_mut(|(id, amount)| {
                                let mut deleted = false;

                                ui.vertical(|ui| {
                                    let icon =
                                        if let Some(module_proto) = self.ctx.modules.get(&id.0) {
                                            ui.add_sized(
                                                [35.0, 35.0],
                                                Icon {
                                                    ctx: self.ctx,
                                                    type_name: "item",
                                                    item_name: &id.0,
                                                    size: 32.0,
                                                    quality: id.1,
                                                },
                                            )
                                            .on_hover_text(self.ctx.get_display_name("item", &id.0))
                                        } else {
                                            ui.add_sized(
                                                [35.0, 35.0],
                                                Icon {
                                                    ctx: self.ctx,
                                                    type_name: "item",
                                                    item_name: "item-unknown",
                                                    size: 32.0,
                                                    quality: 0,
                                                },
                                            )
                                            .on_hover_text("未选择插件")
                                        }
                                        .interact(egui::Sense::click());
                                    if icon.clicked_by(egui::PointerButton::Secondary) {
                                        deleted = true;
                                    }
                                    let mut widget = ItemWithQualitySelectorModal::new(
                                        icon.id,
                                        self.ctx,
                                        "选择插件",
                                        "item",
                                    )
                                    .with_toggle(icon.clicked())
                                    .with_current(id)
                                    .with_filter(|s, f| {
                                        if let Some(module_proto) = f.modules.get(s) {
                                            // 过滤掉不符合要求的插件
                                            beacon_proto.allowed_module_categories.as_ref().is_none_or(
                                                |allowed_categories| {
                                                    allowed_categories
                                                        .contains(&module_proto.category)
                                                },
                                            ) && module_effects_allowed(
                                                module_proto,
                                                &beacon_proto.allowed_effects,
                                            )
                                        } else {
                                            false
                                        }
                                    });
                                    if let Some(changed) = &mut self.changed {
                                        widget = widget.notify_change(changed);
                                    }
                                    ui.add(widget);
                                    let beacon_module_count = beacon_proto.module_slots as usize
                                        + if beacon_proto.quality_affects_module_slots {
                                            let quality_bonus = self.ctx.qualities
                                                [beacon_config.beacon.1 as usize]
                                                .beacon_module_slots_bonus();
                                            quality_bonus as usize
                                        } else {
                                            0
                                        };
                                    let amount_widget = ui.add_sized(
                                        [35.0, 15.0],
                                        egui::DragValue::new(amount)
                                            .range(
                                                0..=(beacon_module_count
                                                    * beacon_config.count as usize
                                                    - total_modules),
                                            )
                                            .clamp_existing_to_range(true)
                                            .speed(1),
                                    );
                                    if amount_widget.changed() {
                                        if let Some(changed) = &mut self.changed {
                                            **changed = true;
                                        }
                                    }

                                    total_modules += *amount;
                                });
                                !deleted
                            });
                            if ui.button("添加插件").clicked() {
                                beacon_config
                                    .modules
                                    .push((IdWithQuality("empty-module-slot".to_string(), 0), 0));
                            }
                        }
                    });
                    !deleted
                });
                if ui.button("添加插件塔").clicked() {
                    self.module_config.beacons.push(BeaconConfig::default());
                }
            });
        });
        ui.response().clone()
    }
}
