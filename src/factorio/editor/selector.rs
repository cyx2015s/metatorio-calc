use std::collections::HashMap;

use egui::Vec2;

use crate::factorio::{
    common::OrderInfo,
    editor::{hover::PrototypeHover, icon::Icon},
    model::context::FactorioContext,
};

#[derive(Debug, Clone, Default)]
pub struct ItemSelectorStorage {
    pub group: usize,
    pub subgroup: usize,
    pub index: usize,
    pub selected_item: Option<String>,
}

pub struct ItemSelector<'a> {
    pub ctx: &'a FactorioContext,
    pub item_type: &'a String,
    pub order_info: &'a OrderInfo,
    pub filter: Box<dyn Fn(&str, &FactorioContext) -> bool + 'a>,
    pub selected_item: &'a mut Option<String>,
}

impl<'a> ItemSelector<'a> {
    pub fn new(
        ctx: &'a FactorioContext,
        item_type: &'a String,
        order_info: &'a OrderInfo,
        selected_item: &'a mut Option<String>,
    ) -> Self {
        Self {
            ctx,
            item_type,
            order_info,
            filter: Box::new(|_, _| true),
            selected_item,
        }
    }

    pub fn with_filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&str, &FactorioContext) -> bool + 'a,
    {
        self.filter = Box::new(filter);
        self
    }
}

impl egui::Widget for ItemSelector<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let mut response = ui.response().clone();
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
        for (i, group) in self.order_info.iter().enumerate() {
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
            storage.index = 0;
            storage.selected_item = None;
        }
        if filtered_group.is_empty() {
            return response;
        }

        egui::Grid::new("ItemGroupGrid")
            .min_row_height(64.0)
            .min_col_width(64.0)
            .max_col_width(64.0)
            .spacing(Vec2 { x: 6.0, y: 6.0 })
            .show(ui, |ui| {
                let mut idx = 0;
                for (i, group) in self.order_info.iter().enumerate() {
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
                            type_name: &"item-group".to_string(),
                            item_name: &group_name,
                            size: 64.0,
                            quality: 0,
                        })
                        .interact(egui::Sense::click())
                        .clicked()
                    {
                        storage.group = i;
                        storage.subgroup = 0;
                        storage.index = 0;
                        storage.selected_item = None;
                    }
                }
            });
        egui::Grid::new("ItemGrid")
            .num_columns(item_count)
            .max_col_width(35.0)
            .min_col_width(35.0)
            .min_row_height(35.0)
            .spacing(Vec2 { x: 0.0, y: 0.0 })
            .striped(true)
            .show(ui, |ui| {
                for (j, subgroup) in self.order_info[storage.group].1.iter().enumerate() {
                    let mut idx = 0;
                    for (k, item_name) in subgroup.1.iter().enumerate() {
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
                                item_name,
                                size: 32.0,
                                quality: 0,
                            })
                            .interact(egui::Sense::click());
                        let button = if self.item_type == &"recipe".to_string() {
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
                            storage.index = k;
                            storage.selected_item = Some(item_name.clone());
                            self.selected_item.replace(item_name.clone());
                        }
                        if storage.subgroup == j && storage.index == k {
                            response = response.union(button);
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
        response
    }
}
