use std::collections::HashMap;

use egui::Vec2;

use crate::factorio::{IdWithQuality, editor::icon::*, model::*};

#[derive(Debug, Clone, Default)]
pub struct SelectorStorage {
    pub group: usize,
    pub subgroup: usize,
}

pub type FilterFn<'a, T> = dyn Fn(&T, &FactorioContext) -> bool + 'a;
pub type HoverUi<'a, T> = dyn Fn(&mut egui::Ui, &T, &FactorioContext) + 'a;

pub struct Selector<'a, Input, Output>
where
    Input: 'a + ?Sized,
{
    pub ctx: &'a FactorioContext,
    pub type_name: &'a str,
    pub filter: Option<Box<FilterFn<'a, Input>>>,
    pub current: Option<&'a mut Output>,
    pub output: Option<&'a mut Option<Output>>,
    pub hover: Option<Box<HoverUi<'a, Input>>>,
    pub forget: bool,
}

impl<'a, Input, Output> Selector<'a, Input, Output>
where
    Input: 'a + ?Sized,
{
    pub fn new(ctx: &'a FactorioContext, type_name: &'a str) -> Self {
        Self {
            ctx,
            type_name,
            filter: None,
            current: None,
            output: None,
            hover: None,
            forget: false,
        }
    }

    pub fn with_current(mut self, selected_item: &'a mut Output) -> Self {
        self.current = Some(selected_item);
        self
    }

    pub fn with_output(mut self, selected_item: &'a mut Option<Output>) -> Self {
        self.output = Some(selected_item);
        self
    }

    pub fn with_filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&Input, &FactorioContext) -> bool + 'a,
    {
        self.filter = Some(Box::new(filter));
        self
    }

    pub fn with_no_filter(mut self) -> Self {
        self.filter = None;
        self
    }

    pub fn chain_filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&Input, &FactorioContext) -> bool + 'a,
    {
        if let Some(prev_filter) = self.filter.take() {
            self.filter = Some(Box::new(move |s, ctx| {
                prev_filter(s, ctx) && filter(s, ctx)
            }));
            return self;
        }
        self.filter = Some(Box::new(filter));
        self
    }

    pub fn with_hover(
        mut self,
        hover: impl Fn(&mut egui::Ui, &Input, &FactorioContext) + 'a,
    ) -> Self {
        self.hover = Some(Box::new(hover));
        self
    }

    pub fn with_forget(mut self, forget: bool) -> Self {
        self.forget = forget;
        self
    }
}

impl<'a> egui::Widget for Selector<'a, str, String> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let mut response = ui.response().clone();
        let available_space = ui.available_size();
        let group_count = (available_space.x as usize / 70).max(4);
        let item_count = (available_space.x as usize / 35).max(8);
        let id = ui.id();
        let mut storage: SelectorStorage =
            ui.memory(move |mem| mem.data.get_temp::<SelectorStorage>(id).unwrap_or_default());
        let mut filtered_group = HashMap::new();
        for (i, group) in self.ctx.ordered_entries[self.type_name].iter().enumerate() {
            for subgroup in group.1.iter() {
                for item_name in subgroup.1.iter() {
                    if !self.filter.as_ref().is_none_or(|f| f(item_name, self.ctx)) {
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
        let order_info = &self.ctx.ordered_entries[self.type_name];
        egui::Grid::new("group")
            .min_row_height(64.0)
            .min_col_width(64.0)
            .max_col_width(64.0)
            .spacing(Vec2 { x: 6.0, y: 6.0 })
            .show(ui, |ui| {
                let mut idx = 0;
                for (i, group) in self.ctx.ordered_entries[self.type_name].iter().enumerate() {
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
                        .add(Icon::new(self.ctx, "item-group", &group_name).with_size(64.0))
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
                        if !self.filter.as_ref().is_none_or(|f| f(item_name, self.ctx)) {
                            continue;
                        }
                        idx += 1;
                        let mut button = ui
                            .add(
                                Icon::new(self.ctx, self.type_name, &item_name.to_string())
                                    .with_size(32.0),
                            )
                            .interact(egui::Sense::click());
                        if let Some(hover) = &self.hover {
                            button = button.on_hover_ui(|ui| (hover)(ui, item_name, self.ctx));
                        } else {
                            button = button.on_hover_text(
                                self.ctx
                                    .get_display_name(self.type_name, item_name)
                                    .to_string(),
                            );
                        }

                        if button.clicked() {
                            storage.subgroup = j;
                            if let Some(&mut ref mut selected_item) = self.current {
                                *selected_item = item_name.clone();
                            }
                            if let Some(&mut ref mut output) = self.output {
                                *output = Some(item_name.clone());
                            }
                            response.mark_changed();
                            response.set_close();
                        }
                    }
                    if idx != 0 {
                        ui.end_row();
                    }
                }
            });
        ui.memory_mut(move |mem| {
            mem.data.insert_temp::<SelectorStorage>(id, storage.clone());
        });
        response
    }
}

#[derive(Debug, Clone, Default)]
pub struct ItemWithQualitySelectorStorage {
    pub selected_item: Option<String>,
    pub selected_quality: Option<u8>,
}

impl<'a> egui::Widget for Selector<'a, IdWithQuality, IdWithQuality> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let id = ui.id();
        let mut response = ui.response().clone();
        let mut storage = if self.forget {
            ItemWithQualitySelectorStorage::default()
        } else {
            ui.memory(|mem| mem.data.get_temp::<ItemWithQualitySelectorStorage>(id))
                .unwrap_or_default()
        };
        let mut selecting_quality = None;
        let mut selecting_item = None;
        quality_selector(ui, self.ctx, &mut selecting_quality);
        let mut widget: Selector<'_, str, String> =
            Selector::new(self.ctx, self.type_name).with_output(&mut selecting_item);
        if let Some(filter) = self.filter {
            widget = widget.with_filter(move |s, f| {
                let id_with_quality =
                    IdWithQuality(s.to_string(), storage.selected_quality.unwrap_or(0));
                filter(&id_with_quality, f)
            });
        }

        if let Some(hover) = self.hover {
            widget = widget.with_hover(move |ui, s, ctx| {
                let id_with_quality =
                    IdWithQuality(s.to_string(), storage.selected_quality.unwrap_or(0));

                hover(ui, &id_with_quality, ctx);
            });
        }
        ui.add(widget);
        if let Some(selected_item) = &selecting_item {
            storage.selected_item = Some(selected_item.clone());
            response.mark_changed();
            if let Some(&mut ref mut current) = self.current {
                current.0 = selected_item.clone();
            }
        }
        if let Some(selected_quality) = selecting_quality {
            storage.selected_quality = Some(selected_quality);
            response.mark_changed();
            if let Some(&mut ref mut current) = self.current {
                current.1 = selected_quality;
            }
        }

        if let (Some(item), Some(quality)) =
            (storage.selected_item.clone(), storage.selected_quality)
        {
            response.mark_changed();
            response.set_close();
            if let Some(&mut ref mut output) = self.output {
                *output = Some(IdWithQuality(item, quality));
            }
        }

        ui.memory_mut(|mem| {
            mem.data
                .insert_temp::<ItemWithQualitySelectorStorage>(id, storage.clone());
        });
        response
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
                        Icon::new(ctx, "quality", &quality.base.name).with_size(32.0),
                    )
                    .on_hover_text(ctx.get_display_name("quality", &quality.base.name))
                    .interact(egui::Sense::click());
                if quality_button.clicked() {
                    *selected_quality = Some(idx as u8);
                }
            }
        });
}
