use std::collections::HashMap;

use egui::Vec2;

use crate::factorio::{
    editor::{hover::PrototypeHover, icon::Icon, modal::show_modal},
    model::context::FactorioContext,
};

#[derive(Debug, Clone, Default)]
pub struct ItemSelectorStorage {
    pub group: usize,
    pub subgroup: usize,
    pub index: usize,
    pub selected_item: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct QualitySelectorStorage {
    pub selected_quality: Option<u8>,
}

pub struct ItemSelector<'a> {
    pub ctx: &'a FactorioContext,
    pub item_type: &'a str,
    pub filter: Box<dyn Fn(&str, &FactorioContext) -> bool + 'a>,
    pub selected_item: &'a mut Option<String>,
}

impl<'a> ItemSelector<'a> {
    pub fn new(
        ctx: &'a FactorioContext,
        item_type: &'a str,
        selected_item: &'a mut Option<String>,
    ) -> Self {
        Self {
            ctx,
            item_type,
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
            storage.index = 0;
            storage.selected_item = None;
        }
        if filtered_group.is_empty() {
            ui.label("无满足条件的选项。");
            return response;
        }
        let order_info = &self.ctx.ordered_entries[self.item_type];
        egui::Grid::new("ItemGroupGrid")
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
                for (j, subgroup) in order_info[storage.group].1.iter().enumerate() {
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

#[derive(Debug, Clone, Default)]
struct FilterString(pub String);

pub fn quality_selector_modal(
    ui: &mut egui::Ui,
    ctx: &FactorioContext,
    label_str: &str,
    button: &egui::Response,
) -> Option<u8> {
    let mut selecting_quality: Option<u8> = None;

    show_modal(button.id, button.clicked(), ui, |ui| {
        ui.label(label_str);
        egui::ScrollArea::vertical()
            .max_width(f32::INFINITY)
            .auto_shrink(false)
            .show(ui, |ui| {
                quality_selector(ui, ctx, &mut selecting_quality);
            });
        if selecting_quality.is_some() {
            ui.close();
        }
    });
    selecting_quality
}

pub fn quality_selector(
    ui: &mut egui::Ui,
    ctx: &FactorioContext,
    selected_quality: &mut Option<u8>,
) {
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
}

pub fn item_selector_modal(
    ui: &mut egui::Ui,
    ctx: &FactorioContext,
    label_str: &str,
    item_type: &str,
    button: &egui::Response,
) -> Option<String> {
    let mut selecting_item = None;

    show_modal(button.id, button.clicked(), ui, |ui| {
        let mut filter_string = ui
            .memory(move |mem| {
                mem.data
                    .get_temp::<FilterString>(button.id)
                    .unwrap_or_default()
            })
            .0;
        ui.label(label_str);
        ui.add(egui::TextEdit::singleline(&mut filter_string).hint_text("筛选器……"));
        egui::ScrollArea::vertical()
            .max_width(f32::INFINITY)
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.add(
                    ItemSelector::new(ctx, item_type, &mut selecting_item).with_filter(|s, f| {
                        if filter_string.is_empty() {
                            return true;
                        }
                        s.to_lowercase().contains(&filter_string.to_lowercase())
                            || f.get_display_name(item_type, s)
                                .to_lowercase()
                                .contains(&filter_string.to_lowercase())
                    }),
                );
            });
        ui.memory_mut(|mem| {
            mem.data.insert_temp(button.id, FilterString(filter_string));
        });
        if selecting_item.is_some() {
            ui.close();
        }
    });
    selecting_item
}

pub fn item_with_quality_selector_modal(
    ui: &mut egui::Ui,
    ctx: &FactorioContext,
    label_str: &str,
    item_type: &str,
    button: &egui::Response,
) -> (Option<String>, Option<u8>) {
    let mut selected_id = None;
    let mut selected_quality = None;
    let id = button.id;

    show_modal(id, button.clicked(), ui, |ui| {
        let mut filter_string = ui
            .memory(move |mem| mem.data.get_temp::<FilterString>(id).unwrap_or_default())
            .0;
        ui.label(label_str);
        ui.horizontal(|ui| {
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
                    selected_quality = Some(idx as u8);
                }
            }
        });
        ui.add(egui::TextEdit::singleline(&mut filter_string).hint_text("筛选器……"));
        egui::ScrollArea::vertical()
            .max_width(f32::INFINITY)
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.add(
                    ItemSelector::new(ctx, item_type, &mut selected_id).with_filter(|s, f| {
                        if filter_string.is_empty() {
                            return true;
                        }
                        s.to_lowercase().contains(&filter_string.to_lowercase())
                            || f.get_display_name(item_type, s)
                                .to_lowercase()
                                .contains(&filter_string.to_lowercase())
                    }),
                );
            });
        ui.memory_mut(|mem| {
            mem.data.insert_temp(id, FilterString(filter_string));
        });
        if selected_id.is_some() {
            ui.close();
        }
    });
    (selected_id, selected_quality)
}
