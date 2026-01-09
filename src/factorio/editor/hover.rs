use egui::Vec2;

use crate::factorio::{common::HasPrototypeBase, editor::icon::Icon, format::{CompactLabel, SignedCompactLabel}, model::{context::Context, recipe::{RecipeIngredient, RecipePrototype, RecipeResult}}};

#[derive(Debug, Clone)]
pub struct PrototypeHover<'a, T: HasPrototypeBase> {
    pub ctx: &'a Context,
    pub prototype: &'a T,
}


impl<'a> egui::Widget for PrototypeHover<'a, RecipePrototype> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let mut ingredients: Vec<&RecipeIngredient> = self.prototype.ingredients.iter().collect();
        ingredients.sort_by_key(|ingredient| match ingredient {
            RecipeIngredient::Item(i) => {
                (0, &self.ctx.reverse_item_order.as_ref().unwrap()[&i.name])
            }
            RecipeIngredient::Fluid(f) => {
                (1, &self.ctx.reverse_fluid_order.as_ref().unwrap()[&f.name])
            }
        });
        let mut results: Vec<&RecipeResult> = self.prototype.results.iter().collect();
        results.sort_by_key(|result| match result {
            RecipeResult::Item(i) => (0, &self.ctx.reverse_item_order.as_ref().unwrap()[&i.name]),
            RecipeResult::Fluid(f) => (1, &self.ctx.reverse_fluid_order.as_ref().unwrap()[&f.name]),
        });
        ui.vertical(|ui| {
            ui.label(
                self.ctx
                    .get_display_name("recipe", &self.prototype.base.name),
            );
            ui.add(CompactLabel::new(self.prototype.energy_required).with_format("{}s"));
            ui.horizontal_top(|ui| {
                if ingredients.is_empty() {
                    ui.label("无原料");
                } else {
                    egui::Grid::new("RecipePrototypeGrid")
                        .min_col_width(35.0)
                        .max_col_width(105.0)
                        .min_row_height(35.0)
                        .spacing(Vec2 { x: 0.0, y: 0.0 })
                        .show(ui, |ui| {
                            for ingredient in ingredients.iter() {
                                match ingredient {
                                    RecipeIngredient::Item(i) => {
                                        let _icon = ui.add(Icon {
                                            ctx: self.ctx,
                                            type_name: &"item".to_string(),
                                            item_name: &i.name,
                                            size: 32.0,
                                            quality: 0,
                                        });
                                        ui.horizontal_top(|ui| {
                                            ui.vertical(|ui| {
                                                ui.add(CompactLabel::new(i.amount));
                                            });
                                        });
                                    }
                                    RecipeIngredient::Fluid(f) => {
                                        let _icon = ui.add(Icon {
                                            ctx: self.ctx,
                                            type_name: &"fluid".to_string(),
                                            item_name: &f.name,
                                            size: 32.0,
                                            quality: 0,
                                        });
                                        ui.vertical(|ui| {
                                            ui.horizontal_top(|ui| {
                                                ui.add(CompactLabel::new(f.amount));
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
                    egui::Grid::new("RecipePrototypeResultGrid")
                        .min_col_width(35.0)
                        .max_col_width(105.0)
                        .min_row_height(35.0)
                        .spacing(Vec2 { x: 0.0, y: 0.0 })
                        .show(ui, |ui| {
                            for result in results.iter() {
                                match result {
                                    RecipeResult::Item(i) => {
                                        let _icon = ui.add(Icon {
                                            ctx: self.ctx,
                                            type_name: &"item".to_string(),
                                            item_name: &i.name,
                                            size: 32.0,
                                            quality: 0,
                                        });
                                        let output = i.normalized_output();
                                        ui.vertical(|ui| {
                                            ui.horizontal_top(|ui| {
                                                ui.style_mut().spacing.item_spacing.x = 0.0;

                                                ui.add(CompactLabel::new(output.0 - output.1));

                                                ui.add(SignedCompactLabel::new(output.1));
                                            });
                                        });
                                    }
                                    RecipeResult::Fluid(f) => {
                                        let _icon = ui.add(Icon {
                                            ctx: self.ctx,
                                            type_name: &"fluid".to_string(),
                                            item_name: &f.name,
                                            size: 32.0,
                                            quality: 0,
                                        });
                                        let output = f.normalized_output();
                                        ui.vertical(|ui| {
                                            ui.horizontal_top(|ui| {
                                                ui.style_mut().spacing.item_spacing.x = 0.0;
                                                ui.add(SignedCompactLabel::new(
                                                    output.0 - output.1,
                                                ));
                                                ui.add(SignedCompactLabel::new(output.1));
                                            });
                                            match f.temperature {
                                                Some(t) => {
                                                    ui.add(
                                                        CompactLabel::new(t).with_format("@{}°C"),
                                                    );
                                                }
                                                None => {
                                                    ui.add(
                                                        CompactLabel::new(
                                                            self.ctx
                                                                .fluids
                                                                .get(&f.name)
                                                                .unwrap()
                                                                .default_temperature,
                                                        )
                                                        .with_format("@{}°C"),
                                                    );
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