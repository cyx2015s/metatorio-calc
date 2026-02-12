use egui::Vec2;

use crate::factorio::{
    icon::*,
    number::{AmountLabel, CompactLabel},
    *,
};

#[derive(Debug, Clone)]
pub struct PrototypeHover<'a, T: HasPrototypeBase> {
    pub data: &'a DataContext,
    pub prototype: &'a T,
    pub quality: u8,
}

impl<'a, T: HasPrototypeBase> PrototypeHover<'a, T> {
    pub fn new(data: &'a DataContext, prototype: &'a T) -> Self {
        Self {
            data,
            prototype,
            quality: 0,
        }
    }

    pub fn with_quality(mut self, quality: u8) -> Self {
        self.quality = quality;
        self
    }
}

impl<'a> egui::Widget for PrototypeHover<'a, RecipePrototype> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let data = &self.data;
        let mut ingredients: Vec<&RecipeIngredient> = self.prototype.ingredients.iter().collect();
        ingredients.sort_by_key(|ingredient| match ingredient {
            RecipeIngredient::Item(i) => (0, data.order_of_entries["item"][&i.name]),
            RecipeIngredient::Fluid(f) => (1, data.order_of_entries["fluid"][&f.name]),
        });
        let mut results: Vec<&RecipeResult> = self.prototype.results.iter().collect();
        results.sort_by_key(|result| match result {
            RecipeResult::Item(i) => (0, data.order_of_entries["item"][&i.name]),
            RecipeResult::Fluid(f) => (1, data.order_of_entries["fluid"][&f.name]),
        });
        ui.vertical(|ui| {
            ui.label(data.get_display_name("recipe", &self.prototype.base.name));
            ui.add(CompactLabel::new(self.prototype.energy_required).with_format("{}s"));
            ui.horizontal_top(|ui| {
                if ingredients.is_empty() {
                    ui.label("无原料");
                } else {
                    egui::Grid::new("recipe")
                        .min_col_width(35.0)
                        .max_col_width(105.0)
                        .min_row_height(35.0)
                        .spacing(Vec2 { x: 0.0, y: 0.0 })
                        .show(ui, |ui| {
                            for ingredient in ingredients.iter() {
                                match ingredient {
                                    RecipeIngredient::Item(i) => {
                                        let _icon = ui.add(Icon::new(self.data, "item", &i.name));
                                        ui.horizontal_top(|ui| {
                                            ui.vertical(|ui| {
                                                ui.add(AmountLabel::new(i.amount));
                                            });
                                        });
                                    }
                                    RecipeIngredient::Fluid(f) => {
                                        let _icon = ui.add(Icon::new(self.data, "fluid", &f.name));
                                        ui.vertical(|ui| {
                                            ui.horizontal_top(|ui| {
                                                ui.add(AmountLabel::new(f.amount));
                                            });
                                            match f.temperature {
                                                Some(t) => {
                                                    ui.label(format!("{}℃", t));
                                                }
                                                None => {
                                                    match (f.min_temperature, f.max_temperature) {
                                                        (Some(min_t), Some(max_t)) => {
                                                            ui.horizontal_top(|ui| {
                                                                ui.add(
                                                                    CompactLabel::new(min_t)
                                                                        .with_format("{}℃"),
                                                                );
                                                                ui.label(" ~ ");
                                                                ui.add(
                                                                    CompactLabel::new(max_t)
                                                                        .with_format("{}℃"),
                                                                );
                                                            });
                                                        }
                                                        (Some(min_t), None) => {
                                                            ui.add(
                                                                CompactLabel::new(min_t)
                                                                    .with_format("≥{}℃"),
                                                            );
                                                        }
                                                        (None, Some(max_t)) => {
                                                            ui.add(
                                                                CompactLabel::new(max_t)
                                                                    .with_format("≤{}℃"),
                                                            );
                                                        }
                                                        (None, None) => {}
                                                    }
                                                }
                                            }
                                        });
                                    }
                                }
                                ui.end_row();
                            }
                        });
                }
                ui.label("→");
                if results.is_empty() {
                    ui.label("无产出");
                    ui.end_row();
                } else {
                    egui::Grid::new("result")
                        .min_col_width(35.0)
                        .max_col_width(105.0)
                        .min_row_height(35.0)
                        .spacing(Vec2 { x: 0.0, y: 0.0 })
                        .show(ui, |ui| {
                            for result in results.iter() {
                                match result {
                                    RecipeResult::Item(i) => {
                                        let _icon = ui.add(Icon::new(self.data, "item", &i.name));
                                        let output = i.normalized_output();
                                        ui.vertical(|ui| {
                                            ui.horizontal_top(|ui| {
                                                ui.style_mut().spacing.item_spacing.x = 0.0;

                                                ui.add(AmountLabel::new(output.0 - output.1));

                                                ui.add(
                                                    AmountLabel::new(output.1).with_is_signed(true),
                                                );
                                            });
                                        });
                                    }
                                    RecipeResult::Fluid(f) => {
                                        let _icon = ui.add(Icon::new(self.data, "fluid", &f.name));
                                        let output = f.normalized_output();
                                        ui.vertical(|ui| {
                                            ui.horizontal_top(|ui| {
                                                ui.style_mut().spacing.item_spacing.x = 0.0;
                                                ui.add(AmountLabel::new(output.0 - output.1));
                                                ui.add(
                                                    AmountLabel::new(output.1).with_is_signed(true),
                                                );
                                            });
                                            match f.temperature {
                                                Some(t) => {
                                                    ui.add(
                                                        CompactLabel::new(t).with_format("@{}°C"),
                                                    );
                                                }
                                                None => {
                                                    if let Some(fluid) = data.fluids.get(&f.name) {
                                                        ui.add(
                                                            CompactLabel::new(
                                                                fluid.default_temperature,
                                                            )
                                                            .with_format("@{}°C"),
                                                        );
                                                    }
                                                }
                                            }
                                        });
                                    }
                                }
                                ui.end_row();
                            }
                        });
                }
            });
        });

        ui.response()
    }
}

impl<'a> egui::Widget for PrototypeHover<'a, CraftingMachinePrototype> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let data = &self.data;
        ui.vertical(|ui| {
            ui.label(data.get_display_name("entity", &self.prototype.base.base.name));
            ui.label(format!("制造速度: {}", self.prototype.crafting_speed));
            ui.label(format!("插件槽位: {}", self.prototype.module_slots));
            ui.label(format!(
                "因为运行而导致的能量消耗: {}W",
                compact_number(
                    self.prototype
                        .energy_usage
                        .as_ref()
                        .map_or(0.0, |e| e.amount)
                        * 60.0
                )
            ));
            if let Some(effect_receiver) = self.prototype.effect_receiver.as_ref() {
                #[allow(irrefutable_let_patterns)]
                if let val = effect_receiver.base_effect.consumption
                    && val != 0.0
                {
                    ui.label(format!("基础能耗: {}%", (val * 100.0) as i32));
                }
                #[allow(irrefutable_let_patterns)]
                if let val = effect_receiver.base_effect.speed
                    && val != 0.0
                {
                    ui.label(format!("基础速度: {}%", (val * 100.0) as i32));
                }
                #[allow(irrefutable_let_patterns)]
                if let val = effect_receiver.base_effect.productivity
                    && val != 0.0
                {
                    ui.label(format!("基础产能: {}%", (val * 100.0) as i32));
                }
                #[allow(irrefutable_let_patterns)]
                if let val = effect_receiver.base_effect.pollution
                    && val != 0.0
                {
                    ui.label(format!("基础污染: {}%", (val * 100.0) as i32));
                }
                #[allow(irrefutable_let_patterns)]
                if let val = effect_receiver.base_effect.quality
                    && val != 0.0
                {
                    ui.label(format!("基础品质: {}%", (val * 100.0) as i32));
                }
            }
        });

        ui.response()
    }
}
