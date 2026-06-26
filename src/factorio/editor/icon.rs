use std::fmt::Display;

use egui::Vec2;

use crate::factorio::{hover::PrototypeHover, *};

#[derive(Debug)]

pub struct Icon<'a> {
    pub data: &'a DataContext,
    pub type_name: &'a str,
    pub item_name: &'a str,
    pub quality: u8,
    pub size: f32,
    pub stroke: egui::Stroke,
}

impl<'a> Icon<'a> {
    pub fn new(data: &'a DataContext, type_name: &'a str, item_name: &'a str) -> Self {
        Self {
            data,
            type_name,
            item_name,
            quality: 0,
            size: 32.0,
            stroke: egui::Stroke::NONE,
        }
    }

    pub fn with_quality(mut self, quality: u8) -> Self {
        self.quality = quality;
        self
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn with_stroke(mut self, stroke: egui::Stroke) -> Self {
        self.stroke = stroke;
        self
    }

    pub fn image(&'_ self) -> egui::Image<'_> {
        let data = &self.data;
        let root_path = &data.icon_path;
        // 某个 type 的 order info 存在，但是没有对应的物品，视为物品不存在
        // 某个 type 的 order info 不存在，当作存在
        let icon_path = if data
            .order_of_entries
            .get(self.type_name)
            .is_some_and(|v| v.get(self.item_name).is_none())
        {
            format!(
                "file://{}/{}/{}.png",
                root_path.to_string_lossy(),
                "item",
                "item-unknown"
            )
        } else {
            format!(
                "file://{}/{}/{}.png",
                root_path.to_string_lossy(),
                self.type_name,
                self.item_name
            )
        };
        egui::Image::new(icon_path)
    }
}

impl<'a> egui::Widget for Icon<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let data = &self.data;
        let root_path = &data.icon_path;
        egui::Frame::NONE
            .fill(egui::Color32::from_rgba_premultiplied(
                0xaa, 0xaa, 0xaa, 0xcc,
            ))
            .corner_radius(4.0)
            .stroke(self.stroke)
            .show(ui, |ui| {
                let mut icon = ui
                    .add(
                        self.image()
                            .max_size(Vec2 {
                                x: self.size,
                                y: self.size,
                            })
                            .maintain_aspect_ratio(true)
                            .shrink_to_fit()
                            .show_loading_spinner(true),
                    )
                    .interact(egui::Sense::click());
                if self.quality > 0 && (self.quality as usize) < data.qualities.len() {
                    ui.put(
                        icon.rect
                            .split_left_right_at_fraction(0.5)
                            .0
                            .split_top_bottom_at_fraction(0.5)
                            .1,
                        egui::Image::new(format!(
                            "file://{}/{}/{}.png",
                            root_path.to_string_lossy(),
                            "quality",
                            data.qualities[self.quality as usize].base.name
                        )),
                    );
                }
                match self.type_name {
                    "item" => {
                        if let Some(item) = data.items.get(self.item_name) {
                            icon = icon.on_hover_ui(|ui| {
                                ui.add(PrototypeHover::new(data, item).with_quality(self.quality));
                            });
                        }
                    }
                    "fluid" => {
                        if let Some(fluid) = data.fluids.get(self.item_name) {
                            icon = icon.on_hover_ui(|ui| {
                                ui.add(PrototypeHover::new(data, fluid));
                            });
                        }
                    }
                    "entity" => {
                        if let Some(entity) = data.entities.get(self.item_name) {
                            icon = icon.on_hover_ui(|ui| {
                                ui.add(
                                    PrototypeHover::new(data, entity).with_quality(self.quality),
                                );
                            });
                        }
                    }
                    "recipe" => {
                        if let Some(recipe) = data.recipes.get(self.item_name) {
                            icon = icon.on_hover_ui(|ui| {
                                ui.add(PrototypeHover::new(data, recipe));
                            });
                        }
                    }
                    "technology" => {
                        if let Some(_technology) = data.technologies.get(self.item_name) {
                            icon = icon
                                .on_hover_text(data.get_display_name("technology", self.item_name))
                        }
                    }
                    "space-location" => {
                        if let Some(planet) = data.planets.get(self.item_name) {
                            icon = icon.on_hover_ui(|ui| {
                                ui.add(PrototypeHover::new(data, planet));
                            })
                        }
                    }
                    "surface" => {
                        if let Some(surface) = data.surfaces.get(self.item_name) {
                            icon = icon.on_hover_ui(|ui| {
                                ui.add(PrototypeHover::new(data, surface));
                            })
                        }
                    }
                    _ => {}
                }
                icon
            })
            .inner
    }
}

#[derive(Debug)]
pub struct GenericIcon<'a> {
    pub data: &'a DataContext,
    pub item: &'a DualVar,
    pub size: f32,
    pub stroke: egui::Stroke,
}

impl<'a> GenericIcon<'a> {
    pub fn new(data: &'a DataContext, item: &'a DualVar) -> Self {
        Self {
            data,
            item,
            size: 32.0,
            stroke: egui::Stroke::NONE,
        }
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn with_stroke(mut self, stroke: egui::Stroke) -> Self {
        self.stroke = stroke;
        self
    }
}

impl<'a> egui::Widget for GenericIcon<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let data = &self.data;
        match self.item {
            DualVar::Custom { name } => ui.label(t!("metatorio.custom", name)),
            DualVar::Item(IdWithQuality(name, quality)) => ui.add_sized(
                [self.size, self.size],
                Icon::new(self.data, "item", name)
                    .with_quality(*quality)
                    .with_size(self.size)
                    .with_stroke(self.stroke),
            ),
            DualVar::Fluid { name, temperature } => {
                let main = ui.add_sized(
                    [self.size, self.size],
                    Icon::new(self.data, "fluid", name)
                        .with_quality(0)
                        .with_size(self.size)
                        .with_stroke(self.stroke),
                );
                let bottom = main.rect.split_top_bottom_at_fraction(0.5).1;
                match temperature {
                    [i32::MIN, i32::MAX] => {}
                    [min, i32::MAX] => {
                        ui.put(
                            bottom,
                            egui::Label::new(
                                egui::RichText::new(format!(">={min}℃"))
                                    .color(egui::Color32::WHITE)
                                    .small(),
                            ),
                        );
                    }
                    [i32::MIN, max] => {
                        ui.put(
                            bottom,
                            egui::Label::new(
                                egui::RichText::new(format!("<={max}℃"))
                                    .color(egui::Color32::WHITE)
                                    .small(),
                            ),
                        );
                    }
                    [min, max] => {
                        if min == max {
                            ui.put(
                                bottom,
                                egui::Label::new(
                                    egui::RichText::new(format!("{min}℃"))
                                        .color(egui::Color32::WHITE)
                                        .small(),
                                )
                                .selectable(false),
                            );
                        } else {
                            ui.put(
                                bottom,
                                egui::Label::new(
                                    egui::RichText::new(format!("{min}~\n{max}℃"))
                                        .color(egui::Color32::WHITE)
                                        .small(),
                                )
                                .selectable(false),
                            );
                        }
                    }
                }

                main
            }
            DualVar::Entity(IdWithQuality(name, quality)) => {
                let main = ui.add_sized(
                    [self.size, self.size],
                    Icon::new(self.data, "entity", name)
                        .with_quality(*quality)
                        .with_size(self.size)
                        .with_stroke(self.stroke),
                );
                let right_bottom = main
                    .rect
                    .split_left_right_at_fraction(0.5)
                    .1
                    .split_top_bottom_at_fraction(0.5)
                    .1;
                ui.put(
                    right_bottom,
                    egui::Label::new(
                        egui::RichText::new("E")
                            .color(egui::Color32::WHITE)
                            .strong(),
                    )
                    .selectable(false),
                );
                main
            }

            DualVar::Heat => {
                egui::Frame::NONE
                    .fill(egui::Color32::from_rgba_premultiplied(
                        0xaa, 0xaa, 0xaa, 0xcc,
                    ))
                    .corner_radius(4.0)
                    .stroke(self.stroke)
                    .show(ui, |ui| {
                        ui.add_sized(
                            [self.size, self.size],
                            egui::Image::new(egui::include_image!(
                                "../../../assets/icons/heat.png"
                            ))
                            .max_size([self.size, self.size].into()),
                        )
                        .on_hover_text(t!("metatorio.heat"))
                    })
                    .inner
            }
            DualVar::Electricity => {
                egui::Frame::NONE
                    .fill(egui::Color32::from_rgba_premultiplied(
                        0xaa, 0xaa, 0xaa, 0xcc,
                    ))
                    .corner_radius(4.0)
                    .stroke(self.stroke)
                    .show(ui, |ui| {
                        ui.add_sized(
                            [self.size, self.size],
                            egui::Image::new(egui::include_image!(
                                "../../../assets/icons/electricity.png"
                            ))
                            .max_size([self.size, self.size].into()),
                        )
                        .on_hover_text(t!("metatorio.electricity"))
                    })
                    .inner
            }
            DualVar::FluidHeat { filter } => match filter {
                Some(fluid) => {
                    ui.add_sized([self.size, self.size], Icon::new(self.data, "fluid", fluid))
                }
                None => {
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgba_premultiplied(
                            0xaa, 0xaa, 0xaa, 0xcc,
                        ))
                        .corner_radius(4.0)
                        .stroke(self.stroke)
                        .show(ui, |ui| {
                            ui.add_sized(
                                [self.size, self.size],
                                egui::Image::new(egui::include_image!(
                                    "../../../assets/icons/fluid-heat.png"
                                ))
                                .max_size([self.size, self.size].into()),
                            )
                            .on_hover_text(t!("metatorio.fluid-heat"))
                        })
                        .inner
                }
            },
            DualVar::FluidFuel { filter } => match filter {
                Some(fluid) => {
                    ui.add_sized([self.size, self.size], Icon::new(self.data, "fluid", fluid))
                }
                None => {
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgba_premultiplied(
                            0xaa, 0xaa, 0xaa, 0xcc,
                        ))
                        .corner_radius(4.0)
                        .stroke(self.stroke)
                        .show(ui, |ui| {
                            ui.add_sized(
                                [self.size, self.size],
                                egui::Image::new(egui::include_image!(
                                    "../../../assets/icons/fluid-fuel.png"
                                ))
                                .max_size([self.size, self.size].into()),
                            )
                            .on_hover_text(t!("metatorio.fluid-fuel"))
                        })
                        .inner
                }
            },
            DualVar::ItemFuel { category } => egui::Frame::NONE
                .fill(egui::Color32::from_rgba_premultiplied(
                    0xaa, 0xaa, 0xaa, 0xcc,
                ))
                .corner_radius(4.0)
                .stroke(self.stroke)
                .show(ui, |ui| {
                    ui.add_sized(
                        [self.size, self.size],
                        egui::Image::new(egui::include_image!(
                            "../../../assets/icons/item-fuel.png"
                        ))
                        .max_size([self.size, self.size].into()),
                    )
                })
                .inner
                .on_hover_text(t!(
                    "metatorio.fuel-category",
                    data.get_display_name("fuel-category", category)
                )),
            DualVar::RocketSlotCapacity => ui
                .add_sized(
                    [self.size, self.size],
                    egui::Image::new(egui::include_image!(
                        "../../../assets/icons/rocket-capacity.png"
                    )),
                )
                .on_hover_ui(|ui| {
                    ui.label(t!("metatorio.rocket-stacks"));
                }),
            DualVar::RocketWeightCapacity => ui
                .add_sized(
                    [self.size, self.size],
                    egui::Image::new(egui::include_image!(
                        "../../../assets/icons/rocket-capacity.png"
                    )),
                )
                .on_hover_ui(|ui| {
                    ui.label(t!("metatorio.by-weight"));
                }),
            DualVar::Pollution { name } => ui
                .add_sized(
                    [self.size, self.size],
                    Icon::new(self.data, "airborne-pollutant", name)
                        .with_size(self.size)
                        .with_stroke(self.stroke),
                )
                .on_hover_ui(|ui| {
                    ui.label(data.get_display_name("airborne-pollutant", name));
                }),
        }
    }
}

impl Display for GenericIcon<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let data = &self.data;
        match self.item {
            DualVar::Custom { name } => write!(f, "{}: {}", t!("metatorio.custom"), name),
            DualVar::Item(IdWithQuality(name, quality)) => {
                write!(
                    f,
                    "{}: {}({})",
                    t!("metatorio.item"),
                    data.get_display_name("item", name),
                    data.get_display_name(
                        "quality",
                        data.qualities
                            .get(*quality as usize)
                            .map_or("unknown", |q| &q.base.name)
                    )
                )
            }
            DualVar::Fluid { name, .. } => {
                write!(
                    f,
                    "{}: {}",
                    t!("metatorio.fluid"),
                    data.get_display_name("fluid", name)
                )
            }
            DualVar::Entity(IdWithQuality(name, quality)) => {
                write!(
                    f,
                    "{}: {}({})",
                    t!("metatorio.entity"),
                    data.get_display_name("entity", name),
                    data.get_display_name(
                        "quality",
                        data.qualities
                            .get(*quality as usize)
                            .map_or("unknown", |q| &q.base.name)
                    )
                )
            }
            DualVar::Heat => f.write_str(t!("metatorio.heat").to_string().as_str()),
            DualVar::Electricity => f.write_str(t!("metatorio.electricity").to_string().as_str()),
            DualVar::FluidHeat { filter } => match filter {
                Some(fluid) => f.write_str(
                    t!(
                        "metatorio.fluid-heat-by",
                        data.get_display_name("fluid", fluid)
                    )
                    .to_string()
                    .as_str(),
                ),
                None => f.write_str(t!("metatorio.fluid-heat-none").to_string().as_str()),
            },
            DualVar::FluidFuel { filter } => match filter {
                Some(fluid) => f.write_str(
                    t!(
                        "metatorio.fluid-fuel-by",
                        data.get_display_name("fluid", fluid)
                    )
                    .to_string()
                    .as_str(),
                ),
                None => f.write_str(t!("metatorio.fluid-fuel-none").to_string().as_str()),
            },
            DualVar::ItemFuel { category } => {
                f.write_str(t!("metatorio.fuel-category", category).to_string().as_str())
            }
            DualVar::RocketSlotCapacity => {
                f.write_str(t!("metatorio.rocket-slot-capacity").to_string().as_str())
            }
            DualVar::RocketWeightCapacity => {
                f.write_str(t!("metatorio.rocket-weight-capacity").to_string().as_str())
            }
            DualVar::Pollution { name } => f.write_str(
                t!(
                    "metatorio.pollution",
                    data.get_display_name("airborne-pollutant", name)
                )
                .to_string()
                .as_str(),
            ),
        }
    }
}