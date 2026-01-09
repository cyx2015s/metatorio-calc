use egui::Vec2;

use crate::factorio::{
        common::HasPrototypeBase,
        model::context::{Context, GenericItem},
    };


#[derive(Debug)]

pub struct Icon<'a> {
    pub ctx: &'a Context,
    pub type_name: &'a String,
    pub item_name: &'a String,
    pub quality: u8,
    pub size: f32,
}

impl<'a> Icon<'a> {
    fn image(&'_ self) -> egui::Image<'_> {
        let icon_path = format!(
            "file://{}/{}/{}.png",
            self.ctx.icon_path.as_ref().unwrap().to_string_lossy(),
            self.type_name,
            self.item_name
        );
        egui::Image::new(icon_path)
    }
}

impl<'a> egui::Widget for Icon<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        egui::Frame::NONE
            .fill(egui::Color32::from_rgba_premultiplied(
                0xaa, 0xaa, 0xaa, 0xcc,
            ))
            .corner_radius(4.0)
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
                if self.quality > 0 {
                    ui.put(
                        icon.rect
                            .split_left_right_at_fraction(0.5)
                            .1
                            .split_top_bottom_at_fraction(0.5)
                            .1,
                        egui::Image::new(format!(
                            "file://{}/{}/{}.png",
                            self.ctx.icon_path.as_ref().unwrap().to_string_lossy(),
                            "quality",
                            self.ctx.qualities[self.quality as usize].base.name
                        )),
                    );
                }
            })
            .response
    }
}

#[derive(Debug)]
pub struct GenericIcon<'a> {
    pub ctx: &'a Context,
    pub item: &'a GenericItem,
    pub size: f32,
}

impl<'a> egui::Widget for GenericIcon<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        match self.item {
            GenericItem::Custom { name } => ui.label(format!("特殊: {}", name)),
            GenericItem::Item { name, quality } => ui
                .add_sized(
                    [self.size, self.size],
                    Icon {
                        ctx: self.ctx,
                        type_name: &"item".to_string(),
                        item_name: name,
                        size: self.size,
                        quality: *quality,
                    },
                )
                .on_hover_text(format!("物品: {}", self.ctx.get_display_name("item", name))),
            GenericItem::Fluid {
                name,
                temperature: _,
            } => ui
                .add_sized(
                    [self.size, self.size],
                    Icon {
                        ctx: self.ctx,
                        type_name: &"fluid".to_string(),
                        item_name: name,
                        size: self.size,
                        quality: 0,
                    },
                )
                .on_hover_text(format!(
                    "流体: {}",
                    self.ctx.get_display_name("fluid", name)
                )),
            GenericItem::Entity { name, quality } => ui
                .add_sized(
                    [self.size, self.size],
                    Icon {
                        ctx: self.ctx,
                        type_name: &"entity".to_string(),
                        item_name: name,
                        size: self.size,
                        quality: *quality,
                    },
                )
                .on_hover_text(format!(
                    "实体: {}",
                    self.ctx.get_display_name("entity", name)
                )),
            GenericItem::Heat => ui.add_sized([self.size, self.size], egui::Label::new("热量")),
            GenericItem::Electricity => {
                ui.add_sized([self.size, self.size], egui::Label::new("电力"))
            }
            GenericItem::FluidHeat { filter } => ui
                .add_sized([self.size, self.size], egui::Label::new("液热"))
                .on_hover_text(format!(
                    "过滤器: {}",
                    filter
                        .as_ref()
                        .map(|f| self.ctx.get_display_name("fluid", f))
                        .unwrap_or("无".to_string())
                )),
            GenericItem::FluidFuel { filter } => ui
                .add_sized([self.size, self.size], egui::Label::new("液燃"))
                .on_hover_text(format!(
                    "过滤器: {}",
                    filter
                        .as_ref()
                        .map(|f| self.ctx.get_display_name("fluid", f))
                        .unwrap_or("无".to_string())
                )),
            GenericItem::ItemFuel { category } => ui
                .add_sized([self.size, self.size], egui::Label::new("物燃".to_string()))
                .on_hover_text(format!("类别: {}", category,)),
            GenericItem::RocketPayloadWeight => {
                ui.add_sized([self.size, self.size], egui::Label::new("重量"))
            }
            GenericItem::RocketPayloadStack => {
                ui.add_sized([self.size, self.size], egui::Label::new("堆叠"))
            }
            GenericItem::Pollution { name } => ui
                .add_sized(
                    [self.size, self.size],
                    egui::Label::new(self.ctx.get_display_name("airborne-pollutant", name)),
                )
                .on_hover_text(format!(
                    "污染物: {}",
                    self.ctx.get_display_name("airborne-pollutant", name)
                )),
        }
    }
}

