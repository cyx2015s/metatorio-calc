use egui::Vec2;

use crate::factorio::{
    icon::*,
    number::{AmountLabel, CompactLabel},
    *,
};

#[derive(Debug, Clone)]
pub struct PrototypeHover<'a, T> {
    pub data: &'a DataContext,
    pub prototype: &'a T,
    pub quality: u8,
}

impl<'a, T> PrototypeHover<'a, T> {
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

impl<'a> egui::Widget for PrototypeHover<'a, ItemPrototype> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let data = &self.data;

        ui.vertical(|ui| {
            ui.set_min_width(140.0);
            ui.label(data.get_display_name("item", &self.prototype.base.name));

            ui.label(format!("单组堆叠: {}", self.prototype.stack_size));

            if let Some(module) = data.modules.get(&self.prototype.base.name) {
                ui.add(PrototypeHover::new(data, module).with_quality(self.quality));
            }
            if let Some(mine) = &self.prototype.burn {
                ui.label(format!("燃料: {}", mine.fuel_value));
                ui.label(format!(
                    "燃料类别: {}",
                    mine.fuel_category.clone().unwrap_or("chemical".to_string())
                ));
            }
            if let Some(place_result) = &self.prototype.place_result {
                ui.label("放置结果: ");
                ui.horizontal(|ui| {
                    ui.label(data.get_display_name("entity", place_result));
                    ui.add_sized([35.0, 35.0], Icon::new(self.data, "entity", place_result));
                });
            }
            if let Some(plant) = &self.prototype.plant {
                ui.label("种植结果: ");
                ui.horizontal(|ui| {
                    ui.label(data.get_display_name("entity", &plant.plant_result));
                    ui.add_sized(
                        [35.0, 35.0],
                        Icon::new(self.data, "entity", &plant.plant_result),
                    );
                });
            }
            if let Some(spoil) = &self.prototype.spoil
                && let Some(spoil_result) = &spoil.spoil_result
            {
                ui.label("变质结果: ");
                ui.horizontal(|ui| {
                    ui.label(data.get_display_name("item", spoil_result));
                    ui.add_sized([35.0, 35.0], Icon::new(self.data, "item", spoil_result));
                });
            }
            if !self.prototype.rocket_launch_products.is_empty() {
                ui.label("火箭发射产物: ");
                for product in &self.prototype.rocket_launch_products {
                    ui.horizontal(|ui| {
                        ui.label(data.get_display_name("item", &product.name));
                        ui.add_sized([35.0, 35.0], Icon::new(self.data, "item", &product.name));
                    });
                }
            }
        });

        ui.response()
    }
}

impl<'a> egui::Widget for PrototypeHover<'a, ModulePrototype> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let data = &self.data;

        ui.vertical(|ui| {
            ui.set_min_width(140.0);

            let effect = effects_under_quality(
                &self.prototype.effect,
                data.qualities[self.quality.min((data.qualities.len() - 1) as u8) as usize]
                    .default_multiplier(),
            );
            ui.label(format!("能耗: {:.0}%", effect.consumption * 100.0));
            ui.label(format!("速度: {:.0}%", effect.speed * 100.0));
            ui.label(format!("产能: {:.0}%", effect.productivity * 100.0));
            ui.label(format!("污染: {:.0}%", effect.pollution * 100.0));
            ui.label(format!("品质: {:.0}%", effect.quality * 100.0));
        });

        ui.response()
    }
}

impl<'a> egui::Widget for PrototypeHover<'a, FluidPrototype> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let data = &self.data;
        ui.vertical(|ui| {
            ui.set_min_width(140.0);
            ui.label(data.get_display_name("fluid", &self.prototype.base.name));
            ui.label(format!("默认温度: {}℃", self.prototype.default_temperature));
            ui.label(format!(
                "最大温度: {}℃",
                self.prototype
                    .max_temperature
                    .unwrap_or(self.prototype.default_temperature)
            ));
            if let Some(fuel_value) = self.prototype.fuel_value {
                ui.label(format!("每单位燃料值: {}", fuel_value));
            }
            if let Some(heat_capacity) = self.prototype.heat_capacity {
                ui.label(format!("每单位比热容: {}/℃", heat_capacity));
            } else {
                ui.label("每单位比热容: 1kJ/℃");
            }
        });

        ui.response()
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
            surface_condition_ui(ui, &self.prototype.surface_conditions, data);
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
                                                    match (
                                                        f.minimum_temperature,
                                                        f.maximum_temperature,
                                                    ) {
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
                                                        CompactLabel::new(t).with_format("@{}℃"),
                                                    );
                                                }
                                                None => {
                                                    if let Some(fluid) = data.fluids.get(&f.name) {
                                                        ui.add(
                                                            CompactLabel::new(
                                                                fluid.default_temperature,
                                                            )
                                                            .with_format("@{}℃"),
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

impl<'a> egui::Widget for PrototypeHover<'a, EntityPrototype> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let data = &self.data;
        ui.vertical(|ui| {
            ui.set_min_width(140.0);
            ui.label(data.get_display_name("entity", &self.prototype.base.name));
            surface_condition_ui(ui, &self.prototype.surface_conditions, data);
            if let Some(mining) = &self.prototype.minable {
                if let Some(result) = &mining.result {
                    ui.label("挖掘返还: ");
                    ui.horizontal(|ui| {
                        ui.label(data.get_display_name("item", result));
                        ui.add_sized([35.0, 35.0], Icon::new(self.data, "item", result));
                        ui.label(format!("×{}", mining.count.unwrap_or(1.0)));
                    });
                } else if !mining.results.is_empty() {
                    ui.label("挖掘返还");
                    for result in &mining.results {
                        match result {
                            RecipeResult::Item(i) => {
                                ui.horizontal(|ui| {
                                    ui.label(data.get_display_name("item", &i.name));
                                    ui.add_sized(
                                        [35.0, 35.0],
                                        Icon::new(self.data, "item", &i.name),
                                    );
                                    let output = i.normalized_output();
                                    ui.label(format!("×{}", output.0));
                                });
                            }
                            RecipeResult::Fluid(f) => {
                                ui.horizontal(|ui| {
                                    ui.label(data.get_display_name("fluid", &f.name));
                                    ui.add_sized(
                                        [35.0, 35.0],
                                        Icon::new(self.data, "fluid", &f.name),
                                    );
                                    let output = f.normalized_output();
                                    ui.label(format!("×{}", output.0));
                                });
                            }
                        }
                    }
                }
                if let Some(required_fluid) = &mining.required_fluid {
                    ui.label("开采流体: ");
                    ui.horizontal(|ui| {
                        ui.label(data.get_display_name("fluid", required_fluid));
                        ui.add_sized([35.0, 35.0], Icon::new(self.data, "fluid", required_fluid));
                        ui.label(format!("×{}", mining.fluid_amount.unwrap() / 10.0));
                    });
                }
                if self.prototype.base.r#type == "resource" {
                    ui.label(format!("挖掘工时: {}%", mining.mining_time * 100.0));
                } else {
                    ui.label(format!("挖掘时间: {}s", mining.mining_time));
                }
            }
            if let Some(miner) = data.miners.get(&self.prototype.base.name) {
                ui.add(PrototypeHover::new(data, miner).with_quality(self.quality));
            }
            if let Some(crafter) = data.crafters.get(&self.prototype.base.name) {
                ui.add(PrototypeHover::new(data, crafter).with_quality(self.quality));
            }
            if let Some(generator) = data.generators.get(&self.prototype.base.name) {
                ui.add(PrototypeHover::new(data, generator).with_quality(self.quality));
            }
        });

        ui.response()
    }
}

impl<'a> egui::Widget for PrototypeHover<'a, MiningDrillPrototype> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let data = &self.data;
        ui.vertical(|ui| {
            ui.set_min_width(140.0);
            ui.label(format!("挖掘速度: {}", self.prototype.mining_speed));
            ui.label(format!(
                "资源消耗: {}%",
                (self
                    .prototype
                    .resource_drain_rate_percent
                    .unwrap_or(100.0)
                    .floor()
                    * data.qualities[self.quality.min((data.qualities.len() - 1) as u8) as usize]
                        .mining_drill_resource_drain_multiplier()) as i32
            ));
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
                effect_receiver_ui(ui, effect_receiver);
            }
        });

        ui.response()
    }
}

impl<'a> egui::Widget for PrototypeHover<'a, CraftingMachinePrototype> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        ui.vertical(|ui| {
            ui.set_min_width(140.0);
            let quality = self.quality.min((self.data.qualities.len() - 1) as u8) as usize;
            ui.label(format!(
                "制造速度: {}",
                self.prototype.crafting_speed
                    * if let Some(mul) = &self.prototype.crafting_speed_quality_multiplier {
                        mul[&self.data.qualities[quality].base.name]
                    } else {
                        self.data.qualities[quality].crafting_machine_speed_multiplier()
                    }
            ));
            ui.label(format!(
                "插件槽位: {}",
                self.prototype.module_slots as i32
                    + if self.prototype.quality_affects_module_slots {
                        self.data.qualities[quality].crafting_machine_module_slots_bonus() as i32
                    } else {
                        0
                    }
            ));
            ui.label(format!(
                "因为运行而导致的能量消耗: {}W",
                compact_number(
                    self.prototype
                        .energy_usage
                        .as_ref()
                        .map_or(0.0, |e| e.amount)
                        * 60.0
                        * (if self.prototype.quality_affects_energy_usage {
                            if let Some(mul) = &self.prototype.energy_usage_quality_multiplier {
                                mul[&self.data.qualities[quality].base.name]
                            } else {
                                self.data.qualities[quality]
                                    .crafting_machine_energy_usage_multiplier()
                            }
                        } else {
                            1.0
                        })
                )
            ));
            if let Some(effect_receiver) = self.prototype.effect_receiver.as_ref() {
                effect_receiver_ui(ui, effect_receiver);
            }
        });

        ui.response()
    }
}

fn effect_receiver_ui(ui: &mut egui::Ui, effect_receiver: &EffectReceiver) {
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

fn surface_condition_ui(
    ui: &mut egui::Ui,
    surface_conditions: &[SurfaceCondition],
    data: &DataContext,
) {
    if surface_conditions.is_empty() {
        return;
    }
    ui.vertical(|ui| {
        ui.label("表面属性限制".to_string());
        ui.horizontal(|ui| {
            for condition in surface_conditions {
                match (condition.min, condition.max) {
                    (Some(min), Some(max)) => {
                        ui.label(format!(
                            "{}: {} ~ {}",
                            data.get_display_name("surface-property", &condition.property),
                            min,
                            max
                        ));
                    }
                    (Some(min), None) => {
                        ui.label(format!(
                            "{}: >={}",
                            data.get_display_name("surface-property", &condition.property),
                            min
                        ));
                    }
                    (None, Some(max)) => {
                        ui.label(format!(
                            "{}: <={}",
                            data.get_display_name("surface-property", &condition.property),
                            max
                        ));
                    }
                    (None, None) => {}
                }
            }
        });
    });
}

impl<'a> egui::Widget for PrototypeHover<'a, GeneratorPrototype> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let data = &self.data;
        ui.vertical(|ui| {
            ui.label(format!(
                "效率: {}%",
                (self.prototype.effectivity * 100.0) as i32
            ));
            if let Some(filter) = &self.prototype.fluid_box.filter {
                ui.add(Icon::new(data, "fluid", filter));
                ui.label(data.get_display_name("fluid", filter));
                if let Some(fluid) = data.fluids.get(filter) {
                    let flow = self
                        .prototype
                        .get_output(fluid, self.prototype.maximum_temperature);
                    ui.label(format!("流体消耗: {}/s", flow.0));
                    ui.label(format!("电量输出: {}", EnergyAmount { amount: flow.1 }));
                }
            }
        });

        ui.response()
    }
}

impl<'a> egui::Widget for PrototypeHover<'a, PlanetPrototype> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let planet = self.prototype;
        ui.label(
            self.data
                .get_display_name("space-location", &planet.base.name),
        );
        for property in &self.data.surface_properties {
            if let Some(value) = planet.surface_properties.get(property.0) {
                ui.label(format!(
                    "{}: {}",
                    self.data.get_display_name("surface-property", property.0),
                    value
                ));
            } else {
                ui.label(format!(
                    "{}: {}",
                    self.data.get_display_name("surface-property", property.0),
                    property.1.default_value
                ));
            }
        }
        ui.response()
    }
}
