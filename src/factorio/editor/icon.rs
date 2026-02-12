use std::fmt::Display;

use egui::Vec2;

use crate::factorio::*;

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
                let icon = ui.add(
                    self.image()
                        .max_size(Vec2 {
                            x: self.size,
                            y: self.size,
                        })
                        .maintain_aspect_ratio(true)
                        .shrink_to_fit()
                        .show_loading_spinner(true),
                );
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
            })
            .response
    }
}

#[derive(Debug)]
pub struct GenericIcon<'a> {
    pub data: &'a DataContext,
    pub item: &'a GenericItem,
    pub size: f32,
    pub stroke: egui::Stroke,
}

impl<'a> GenericIcon<'a> {
    pub fn new(data: &'a DataContext, item: &'a GenericItem) -> Self {
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
            GenericItem::Custom { name } => ui.label(format!("特殊: {}", name)),
            GenericItem::Item(IdWithQuality(name, quality)) => ui.add_sized(
                [self.size, self.size],
                Icon::new(self.data, "item", name)
                    .with_quality(*quality)
                    .with_size(self.size)
                    .with_stroke(self.stroke),
            ),
            GenericItem::Fluid {
                name,
                temperature: _,
            } => ui.add_sized(
                [self.size, self.size],
                Icon::new(self.data, "fluid", name)
                    .with_quality(0)
                    .with_size(self.size)
                    .with_stroke(self.stroke),
            ),
            GenericItem::Entity(IdWithQuality(name, quality)) => {
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

            GenericItem::Heat => ui.add_sized([self.size, self.size], egui::Label::new("热量")),
            GenericItem::Electricity => {
                ui.add_sized([self.size, self.size], egui::Label::new("电能"))
            }
            GenericItem::FluidHeat { filter } => match filter {
                Some(fluid) => {
                    ui.add_sized([self.size, self.size], Icon::new(self.data, "fluid", fluid))
                }
                None => ui.add_sized([self.size, self.size], egui::Label::new("液热")),
            },
            GenericItem::FluidFuel { filter } => match filter {
                Some(fluid) => {
                    ui.add_sized([self.size, self.size], Icon::new(self.data, "fluid", fluid))
                }
                None => ui.add_sized([self.size, self.size], egui::Label::new("液燃")),
            },
            GenericItem::ItemFuel { category } => ui
                .add_sized([self.size, self.size], egui::Label::new("物燃".to_string()))
                .on_hover_text(format!("类别: {}", category,)),
            GenericItem::RocketPayloadWeight => {
                ui.add_sized([self.size, self.size], egui::Label::new("重量"))
            }
            GenericItem::RocketPayloadStack => {
                ui.add_sized([self.size, self.size], egui::Label::new("堆叠"))
            }
            GenericItem::Pollution { name } => ui.add_sized(
                [self.size, self.size],
                egui::Label::new(data.get_display_name("airborne-pollutant", name)),
            ),
        }
    }
}

impl Display for GenericIcon<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let data = &self.data;
        match self.item {
            GenericItem::Custom { name } => write!(f, "特殊: {}", name),
            GenericItem::Item(IdWithQuality(name, quality)) => {
                write!(
                    f,
                    "物品: {}({})",
                    data.get_display_name("item", name),
                    data.get_display_name(
                        "quality",
                        data.qualities
                            .get(*quality as usize)
                            .map_or("unknown", |q| &q.base.name)
                    )
                )
            }
            GenericItem::Fluid { name, .. } => {
                write!(f, "流体: {}", data.get_display_name("fluid", name))
            }
            GenericItem::Entity(IdWithQuality(name, quality)) => {
                write!(
                    f,
                    "实体: {}({})",
                    data.get_display_name("entity", name),
                    data.get_display_name(
                        "quality",
                        data.qualities
                            .get(*quality as usize)
                            .map_or("unknown", |q| &q.base.name)
                    )
                )
            }
            GenericItem::Heat => write!(f, "热量"),
            GenericItem::Electricity => write!(f, "电能"),
            GenericItem::FluidHeat { filter } => match filter {
                Some(fluid) => write!(
                    f,
                    "通过热交换 {} 获得能量",
                    data.get_display_name("fluid", fluid)
                ),
                None => write!(f, "任意来源的流体热量"),
            },
            GenericItem::FluidFuel { filter } => match filter {
                Some(fluid) => write!(
                    f,
                    "通过燃烧 {} 获得的能量",
                    data.get_display_name("fluid", fluid)
                ),
                None => write!(f, "任意来源的流体燃料"),
            },
            GenericItem::ItemFuel { category } => {
                write!(f, "燃料类别: {}", category)
            }
            GenericItem::RocketPayloadWeight => write!(f, "重量载荷"),
            GenericItem::RocketPayloadStack => write!(f, "堆叠载荷"),
            GenericItem::Pollution { name } => {
                write!(
                    f,
                    "污染物: {}",
                    data.get_display_name("airborne-pollutant", name)
                )
            }
        }
    }
}
