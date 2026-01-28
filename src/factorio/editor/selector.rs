use std::collections::HashMap;

use egui::Vec2;

use crate::factorio::{IdWithQuality, editor::icon::*, hover::*, model::*};

#[derive(Debug, Clone, Default)]
pub struct ItemSelectorStorage {
    pub group: usize,
    pub subgroup: usize,
}

pub struct ItemSelector<'a> {
    pub ctx: &'a FactorioContext,
    pub item_type: &'a str,
    pub filter: Box<dyn Fn(&str, &FactorioContext) -> bool + 'a>,
    pub current: Option<&'a mut String>,
    pub output: Option<&'a mut Option<String>>,
}

impl<'a> ItemSelector<'a> {
    pub fn new(ctx: &'a FactorioContext, item_type: &'a str) -> Self {
        Self {
            ctx,
            item_type,
            filter: Box::new(|_, _| true),
            current: None,
            output: None,
        }
    }

    pub fn with_current(mut self, selected_item: &'a mut String) -> Self {
        self.current = Some(selected_item);
        self
    }

    pub fn with_output(mut self, selected_item: &'a mut Option<String>) -> Self {
        self.output = Some(selected_item);
        self
    }

    pub fn with_filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&str, &FactorioContext) -> bool + 'a,
    {
        self.filter = Box::new(filter);
        self
    }

    pub fn chain_filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&str, &FactorioContext) -> bool + 'a,
    {
        let previous_filter = self.filter;
        self.filter = Box::new(move |s, ctx| previous_filter(s, ctx) && filter(s, ctx));
        self
    }
}

impl egui::Widget for ItemSelector<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let available_space = ui.available_size();
        let group_count = (available_space.x as usize / 70).max(4);
        let item_count = (available_space.x as usize / 35).max(8);
        let id = ui.id();
        let mut storage: ItemSelectorStorage = ui.memory(move |mem| {
            mem.data
                .get_temp::<ItemSelectorStorage>(id)
                .unwrap_or_default()
        });
        let mut filtered_group = HashMap::new();
        for (i, group) in self.ctx.ordered_entries[self.item_type].iter().enumerate() {
            for subgroup in group.1.iter() {
                for item_name in subgroup.1.iter() {
                    if !(self.filter)(item_name, self.ctx) {
                        continue;
                    }
                    filtered_group.insert(i, true);
                    break;
                }
            }
        }
        if !filtered_group.contains_key(&storage.group) {
            storage.group = filtered_group.iter().next().map(|(k, _)| *k).unwrap_or(0);
            storage.subgroup = 0;
        }
        if filtered_group.is_empty() {
            ui.label("无满足条件的选项。");
            return ui.response().clone();
        }
        let order_info = &self.ctx.ordered_entries[self.item_type];
        egui::Grid::new("group")
            .min_row_height(64.0)
            .min_col_width(64.0)
            .max_col_width(64.0)
            .spacing(Vec2 { x: 6.0, y: 6.0 })
            .show(ui, |ui| {
                let mut idx = 0;
                for (i, group) in self.ctx.ordered_entries[self.item_type].iter().enumerate() {
                    if (idx % group_count) == 0 && idx != 0 {
                        ui.end_row();
                    }
                    let group_name = if group.0.is_empty() {
                        "other".to_string()
                    } else {
                        group.0.clone()
                    };
                    if !filtered_group.contains_key(&i) {
                        continue;
                    }
                    idx += 1;
                    if ui
                        .add(Icon {
                            ctx: self.ctx,
                            type_name: "item-group",
                            item_name: &group_name,
                            size: 64.0,
                            quality: 0,
                        })
                        .interact(egui::Sense::click())
                        .clicked()
                    {
                        storage.group = i;
                        storage.subgroup = 0;
                    }
                }
            });
        egui::Grid::new("item")
            .num_columns(item_count)
            .max_col_width(35.0)
            .min_col_width(35.0)
            .min_row_height(35.0)
            .spacing(Vec2 { x: 0.0, y: 0.0 })
            .striped(true)
            .show(ui, |ui| {
                for (j, subgroup) in order_info[storage.group].1.iter().enumerate() {
                    let mut idx = 0;
                    for item_name in subgroup.1.iter() {
                        if (idx % item_count) == 0 && idx != 0 {
                            ui.end_row();
                        }
                        if !(self.filter)(item_name, self.ctx) {
                            continue;
                        }
                        idx += 1;
                        let button = ui
                            .add(Icon {
                                ctx: self.ctx,
                                type_name: self.item_type,
                                item_name: &item_name.to_string(),
                                size: 32.0,
                                quality: 0,
                            })
                            .interact(egui::Sense::click());
                        let button = if self.item_type == "recipe" {
                            let prototype = self.ctx.recipes.get(item_name).unwrap();
                            button.on_hover_ui(|ui| {
                                ui.add(PrototypeHover {
                                    ctx: self.ctx,
                                    prototype,
                                });
                            })
                        } else {
                            button
                                .on_hover_text(self.ctx.get_display_name(self.item_type, item_name))
                        };

                        if button.clicked() {
                            storage.subgroup = j;
                            if let Some(&mut ref mut selected_item) = self.current {
                                *selected_item = item_name.clone();
                            }
                            if let Some(&mut ref mut output) = self.output {
                                *output = Some(item_name.clone());
                            }
                        }
                    }
                    if idx != 0 {
                        ui.end_row();
                    }
                }
            });
        ui.memory_mut(move |mem| {
            mem.data
                .insert_temp::<ItemSelectorStorage>(id, storage.clone());
        });
        ui.response().clone()
    }
}

pub struct ItemWithQualitySelector<'a> {
    pub ctx: &'a FactorioContext,
    pub item_type: &'a str,
    pub filter: Box<dyn Fn(&str, &FactorioContext) -> bool + 'a>,
    pub current: Option<&'a mut IdWithQuality>,
    pub output: Option<&'a mut Option<IdWithQuality>>,
    pub forget: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ItemWithQualitySelectorStorage {
    pub selected_item: Option<String>,
    pub selected_quality: Option<u8>,
}

impl<'a> ItemWithQualitySelector<'a> {
    pub fn new(ctx: &'a FactorioContext, item_type: &'a str) -> Self {
        Self {
            ctx,
            item_type,
            filter: Box::new(|_, _| true),
            current: None,
            output: None,
            forget: false,
        }
    }

    pub fn with_current(mut self, selected_item: &'a mut IdWithQuality) -> Self {
        self.current = Some(selected_item);
        self
    }

    pub fn with_output(mut self, output: &'a mut Option<IdWithQuality>) -> Self {
        self.output = Some(output);
        self
    }

    pub fn with_forget(mut self, forget: bool) -> Self {
        self.forget = forget;
        self
    }

    pub fn with_filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&str, &FactorioContext) -> bool + 'a,
    {
        self.filter = Box::new(filter);
        self
    }

    pub fn chain_filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&str, &FactorioContext) -> bool + 'a,
    {
        let previous_filter = self.filter;
        self.filter = Box::new(move |s, ctx| previous_filter(s, ctx) && filter(s, ctx));
        self
    }
}

impl<'a> egui::Widget for ItemWithQualitySelector<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let id = ui.id();

        let mut storage = if self.forget {
            ItemWithQualitySelectorStorage::default()
        } else {
            ui.memory(|mem| mem.data.get_temp::<ItemWithQualitySelectorStorage>(id))
                .unwrap_or_default()
        };
        let mut selecting_quality = None;
        let mut selecting_item = None;
        quality_selector(ui, self.ctx, &mut selecting_quality);
        ui.add(
            ItemSelector::new(self.ctx, self.item_type)
                .with_output(&mut selecting_item)
                .with_filter(self.filter),
        );
        if let Some(selected_item) = &selecting_item {
            storage.selected_item = Some(selected_item.clone());
            if let Some(&mut ref mut current) = self.current {
                current.0 = selected_item.clone();
            }
        }
        if let Some(selected_quality) = selecting_quality {
            storage.selected_quality = Some(selected_quality);
            if let Some(&mut ref mut current) = self.current {
                current.1 = selected_quality;
            }
        }
        if let Some(&mut ref mut output) = self.output {
            if let (Some(item), Some(quality)) =
                (storage.selected_item.clone(), storage.selected_quality)
            {
                *output = Some(IdWithQuality(item, quality));
            }
        }

        ui.memory_mut(|mem| {
            mem.data
                .insert_temp::<ItemWithQualitySelectorStorage>(id, storage.clone());
        });

        ui.response().clone()
    }
}

fn quality_selector(ui: &mut egui::Ui, ctx: &FactorioContext, selected_quality: &mut Option<u8>) {
    egui::Grid::new("quality")
        .max_col_width(35.0)
        .min_col_width(35.0)
        .min_row_height(35.0)
        .spacing(Vec2 { x: 0.0, y: 0.0 })
        .show(ui, |ui| {
            for (idx, quality) in ctx.qualities.iter().enumerate() {
                let quality_button = ui
                    .add_sized(
                        [32.0, 32.0],
                        Icon {
                            ctx,
                            type_name: "quality",
                            item_name: &quality.base.name,
                            size: 32.0,
                            quality: 0,
                        },
                    )
                    .on_hover_text(ctx.get_display_name("quality", &quality.base.name))
                    .interact(egui::Sense::click());
                if quality_button.clicked() {
                    *selected_quality = Some(idx as u8);
                }
            }
        });
}
