use std::collections::HashMap;

use egui::Vec2;

use crate::factorio::{
    DataContext, IdWithQuality, drag_value, editor::icon::*, hover::PrototypeHover,
    modal::SelectorModal, model::*,
};

#[derive(Debug, Clone, Default)]
pub struct SelectorStorage {
    pub group: usize,
    pub subgroup: usize,
}

pub type FilterFn<'a, T> = dyn Fn(&T, &DataContext) -> bool + 'a;
pub type HoverUi<'a, T> = dyn Fn(&mut egui::Ui, &T, &DataContext) + 'a;

pub struct Selector<'a, Input, Output>
where
    Input: 'a + ?Sized,
{
    pub data: &'a DataContext,
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
    pub fn new(data: &'a DataContext, type_name: &'a str) -> Self {
        Self {
            data,
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
        F: Fn(&Input, &DataContext) -> bool + 'a,
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
        F: Fn(&Input, &DataContext) -> bool + 'a,
    {
        if let Some(prev_filter) = self.filter.take() {
            self.filter = Some(Box::new(move |s, data| {
                prev_filter(s, data) && filter(s, data)
            }));
            return self;
        }
        self.filter = Some(Box::new(filter));
        self
    }

    pub fn with_hover(mut self, hover: impl Fn(&mut egui::Ui, &Input, &DataContext) + 'a) -> Self {
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
        for (i, group) in self.data.ordered_entries[self.type_name].iter().enumerate() {
            for subgroup in group.1.iter() {
                for item_name in subgroup.1.iter() {
                    if !self.filter.as_ref().is_none_or(|f| f(item_name, self.data)) {
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
        let order_info = &self.data.ordered_entries[self.type_name];
        egui::Grid::new("group")
            .min_row_height(64.0)
            .min_col_width(64.0)
            .max_col_width(64.0)
            .spacing(Vec2 { x: 6.0, y: 6.0 })
            .show(ui, |ui| {
                let mut idx = 0;
                for (i, group) in order_info.iter().enumerate() {
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
                    let widget = Icon::new(self.data, "item-group", &group_name)
                        .with_size(64.0)
                        .with_stroke(if i == storage.group {
                            egui::Stroke::new(2.0, egui::Color32::GRAY)
                        } else {
                            egui::Stroke::NONE
                        });

                    if ui.add(widget).clicked() {
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
                        if !self.filter.as_ref().is_none_or(|f| f(item_name, self.data)) {
                            continue;
                        }
                        if (idx % item_count) == 0 && idx != 0 {
                            ui.end_row();
                        }
                        idx += 1;
                        let mut icon =
                            Icon::new(self.data, self.type_name, item_name).with_size(32.0);
                        if self.current.as_ref().is_some_and(|x| x == &item_name)
                            || self
                                .output
                                .as_ref()
                                .is_some_and(|x| x.as_ref().is_some_and(|y| y == item_name))
                        {
                            icon = icon.with_stroke(egui::Stroke::new(2.0, egui::Color32::GRAY));
                        }
                        let mut button = ui.add(icon);
                        if let Some(hover) = &self.hover {
                            button = button.on_hover_ui(|ui| (hover)(ui, item_name, self.data));
                        } else {
                            match self.type_name {
                                "entity" => {
                                    button = button.on_hover_ui(|ui| {
                                        if let Some(entity) = self.data.entities.get(item_name) {
                                            ui.add(PrototypeHover::new(self.data, entity));
                                        }
                                    });
                                }
                                "item" => {
                                    button = button.on_hover_ui(|ui| {
                                        if let Some(item) = self.data.items.get(item_name) {
                                            ui.add(PrototypeHover::new(self.data, item));
                                        }
                                    });
                                }
                                "fluid" => {
                                    button = button.on_hover_ui(|ui| {
                                        if let Some(fluid) = self.data.fluids.get(item_name) {
                                            ui.add(PrototypeHover::new(self.data, fluid));
                                        }
                                    });
                                }
                                "recipe" => {
                                    button = button.on_hover_ui(|ui| {
                                        if let Some(recipe) = self.data.recipes.get(item_name) {
                                            ui.add(PrototypeHover::new(self.data, recipe));
                                        }
                                    });
                                }
                                _ => {
                                    button = button.on_hover_text(
                                        self.data
                                            .get_display_name(self.type_name, item_name)
                                            .to_string(),
                                    );
                                }
                            }
                        }

                        if button.clicked() {
                            storage.subgroup = j;
                            if let Some(&mut ref mut selected_item) = self.current {
                                *selected_item = item_name.to_string();
                            }
                            if let Some(&mut ref mut output) = self.output {
                                *output = Some(item_name.to_string());
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
        let prev_storage_quality = storage.selected_quality;
        let prev_storage_item = storage.selected_item.clone();
        if quality_selector(ui, self.data, &mut storage.selected_quality) {
            response.mark_changed();
        }
        let mut widget: Selector<'_, str, String> =
            Selector::new(self.data, self.type_name).with_output(&mut storage.selected_item);
        if let Some(filter) = self.filter {
            widget = widget.with_filter(move |s, f| {
                let id_with_quality =
                    IdWithQuality(s.to_string(), storage.selected_quality.unwrap_or(0));
                filter(&id_with_quality, f)
            });
        }

        if let Some(hover) = self.hover {
            widget = widget.with_hover(move |ui, s, data| {
                let id_with_quality =
                    IdWithQuality(s.to_string(), storage.selected_quality.unwrap_or(0));

                hover(ui, &id_with_quality, data);
            });
        } else {
            match self.type_name {
                "entity" => {
                    widget = widget.with_hover(|ui, s, data| {
                        if let Some(entity) = data.entities.get(s) {
                            ui.add(
                                PrototypeHover::new(data, entity)
                                    .with_quality(storage.selected_quality.unwrap_or(0)),
                            );
                        }
                    });
                }
                "item" => {
                    widget = widget.with_hover(|ui, s, data| {
                        if let Some(item) = data.items.get(s) {
                            ui.add(
                                PrototypeHover::new(data, item)
                                    .with_quality(storage.selected_quality.unwrap_or(0)),
                            );
                        }
                    });
                }
                "fluid" => {
                    widget = widget.with_hover(|ui, s, data| {
                        if let Some(fluid) = data.fluids.get(s) {
                            ui.add(PrototypeHover::new(data, fluid));
                        }
                    });
                }
                "recipe" => {
                    widget = widget.with_hover(|ui, s, data| {
                        if let Some(recipe) = data.recipes.get(s) {
                            ui.add(PrototypeHover::new(data, recipe));
                        }
                    });
                }
                _ => {
                    widget = widget.with_hover(|ui, s, data| {
                        ui.label(data.get_display_name(self.type_name, s));
                    })
                }
            }
        }
        if ui.add(widget).changed() {
            response.mark_changed();
        }
        if prev_storage_item != storage.selected_item
            && let Some(selected_item) = &storage.selected_item
        {
            response.mark_changed();
            if let Some(&mut ref mut current) = self.current {
                current.0 = selected_item.clone();
            }
        }
        if prev_storage_quality != storage.selected_quality
            && let Some(selected_quality) = &storage.selected_quality
        {
            response.mark_changed();
            if let Some(&mut ref mut current) = self.current {
                current.1 = *selected_quality;
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

pub fn quality_selector(
    ui: &mut egui::Ui,
    data: &DataContext,
    selected_quality: &mut Option<u8>,
) -> bool {
    let mut changed = false;
    egui::Grid::new("quality")
        .max_col_width(35.0)
        .min_col_width(35.0)
        .min_row_height(35.0)
        .spacing(Vec2 { x: 0.0, y: 0.0 })
        .show(ui, |ui| {
            for (idx, quality) in data.qualities.iter().enumerate() {
                let quality_button = ui.add_sized(
                    [32.0, 32.0],
                    Icon::new(data, "quality", &quality.base.name)
                        .with_size(32.0)
                        .with_stroke(
                            if let Some(quality) = selected_quality
                                && *quality == idx as u8
                            {
                                egui::Stroke::new(2.0, egui::Color32::GRAY)
                            } else {
                                egui::Stroke::NONE
                            },
                        ),
                );
                if quality_button.clicked() {
                    *selected_quality = Some(idx as u8);
                    changed = true;
                }
            }
        });
    changed
}

pub fn generic_item_selector(
    ui: &mut egui::Ui,
    data: &DataContext,
    selected: &mut GenericItem,
    response: &egui::Response,
    id: egui::Id,
) -> bool {
    let mut changed = false;
    let toggle = response.clicked();
    let clear = response.secondary_clicked();
    ui.vertical(|ui| {
        egui::ComboBox::from_id_salt(id)
            .selected_text(selected.to_string())
            .show_ui(ui, |ui| {
                changed |= ui
                    .selectable_value(selected, GenericItem::Item("item-unknown".into()), "物品")
                    .changed();
                changed |= ui
                    .selectable_value(
                        selected,
                        GenericItem::Fluid {
                            name: "fluid-unknown".to_string(),
                            temperature: [i32::MIN, i32::MAX],
                        },
                        "流体",
                    )
                    .changed();
                changed |= ui
                    .selectable_value(
                        selected,
                        GenericItem::Entity("entity-unknown".into()),
                        "实体",
                    )
                    .changed();
                changed |= ui
                    .selectable_value(selected, GenericItem::Heat, "热能")
                    .changed();
                changed |= ui
                    .selectable_value(selected, GenericItem::Electricity, "电能")
                    .changed();
                changed |= ui
                    .selectable_value(
                        selected,
                        GenericItem::FluidHeat { filter: None },
                        "流体热源",
                    )
                    .changed();
                changed |= ui
                    .selectable_value(
                        selected,
                        GenericItem::FluidFuel { filter: None },
                        "流体燃料",
                    )
                    .changed();
                changed |= ui
                    .selectable_value(
                        selected,
                        GenericItem::ItemFuel {
                            category: "chemical".to_string(),
                        },
                        "物体燃料",
                    )
                    .changed();
                changed |= ui
                    .selectable_value(
                        selected,
                        GenericItem::Pollution {
                            name: "pollution".to_string(),
                        },
                        "污染物",
                    )
                    .changed();
            });
        match selected {
            GenericItem::Item(id_with_quality) => {
                changed |= ui
                    .add(
                        SelectorModal::new(id.with("select-item"), data, "选择物品")
                            .with_toggle(toggle)
                            .with_selector(
                                Selector::new(data, "item").with_current(id_with_quality),
                            ),
                    )
                    .changed();
            }
            GenericItem::Fluid { name, temperature } => {
                changed |= ui
                    .add(
                        SelectorModal::new(id.with("select-fluid"), data, "选择流体")
                            .with_toggle(toggle)
                            .with_selector(Selector::new(data, "fluid").with_current(name)),
                    )
                    .changed();
                let [min, max] = temperature;
                if min == max {
                    let mut cur_temp = *min;
                    ui.horizontal(|ui| {
                        changed |= ui.add(drag_value(&mut cur_temp).speed(1)).changed();

                        if ui.button("无温度").clicked() {
                            *temperature = [i32::MIN, i32::MAX];
                            changed = true;
                        } else {
                            *temperature = [cur_temp, cur_temp];
                        }
                    });
                } else if ui.button("附加温度").clicked() {
                    let default = data
                        .fluids
                        .get(name)
                        .map(|f| f.default_temperature)
                        .unwrap_or(15.0) as i32;
                    *temperature = [default, default];
                    changed = true;
                }
            }
            GenericItem::Entity(id_with_quality) => {
                changed |= ui
                    .add(
                        SelectorModal::new(id.with("select-entity"), data, "选择实体")
                            .with_toggle(toggle)
                            .with_selector(
                                Selector::new(data, "entity")
                                    .with_current(id_with_quality)
                                    .with_filter(|s: &IdWithQuality, f| {
                                        f.entities.get(&s.0).is_some_and(|e| {
                                            e.base.r#type == "resource"
                                                || e.base.r#type == "asteroid-chunk"
                                        })
                                    }),
                            ),
                    )
                    .changed();
            }
            GenericItem::Heat => {}
            GenericItem::Electricity => {}
            GenericItem::FluidHeat { filter } => {
                changed |= ui
                    .add(
                        SelectorModal::new(id.with("select-fluid-heat"), data, "选择流体热源来源")
                            .with_toggle(toggle)
                            .with_selector(
                                Selector::new(data, "fluid")
                                    .with_output(filter)
                                    .with_filter(|s, f| {
                                        f.fluids[s]
                                            .heat_capacity
                                            .as_ref()
                                            .is_none_or(|c| c.amount > 0.0)
                                    }),
                            ),
                    )
                    .changed();
                if clear {
                    *filter = None;
                    changed = true;
                }
            }
            GenericItem::FluidFuel { filter } => {
                if clear {
                    *filter = None;
                    changed = true;
                }
                changed |= ui
                    .add(
                        SelectorModal::new(id, data, "选择流体燃料")
                            .with_toggle(toggle)
                            .with_selector(
                                Selector::new(data, "fluid")
                                    .with_output(filter)
                                    .with_filter(|s, f| {
                                        f.fluids[s]
                                            .fuel_value
                                            .as_ref()
                                            .is_some_and(|c| c.amount > 0.0)
                                    }),
                            ),
                    )
                    .changed();
            }
            GenericItem::ItemFuel { category } => {
                egui::ComboBox::from_id_salt(id.with("item-fuel-category"))
                    .selected_text(data.get_display_name("fuel-category", category))
                    .show_ui(ui, |ui| {
                        for cat in data.order_of_entries["fuel-category"].keys() {
                            changed |= ui
                                .selectable_value(
                                    category,
                                    cat.clone(),
                                    data.get_display_name("fuel-category", cat),
                                )
                                .clicked();
                        }
                    });
            }
            GenericItem::Pollution { name } => {
                egui::ComboBox::from_id_salt(id.with("pollution-type"))
                    .selected_text(data.get_display_name("airborne-pollutant", name))
                    .show_ui(ui, |ui| {
                        for pollution in data.order_of_entries["airborne-pollutant"].keys() {
                            changed |= ui
                                .selectable_value(
                                    name,
                                    pollution.clone(),
                                    data.get_display_name("airborne-pollutant", pollution),
                                )
                                .clicked();
                        }
                    });
            }
            _ => {}
        }
    });
    changed
}
