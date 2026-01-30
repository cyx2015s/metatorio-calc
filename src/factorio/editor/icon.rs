use egui::Vec2;

use crate::factorio::*;

#[derive(Debug)]

pub struct Icon<'a> {
    pub ctx: &'a FactorioContext,
    pub type_name: &'a str,
    pub item_name: &'a str,
    pub quality: u8,
    pub size: f32,
}

impl<'a> Icon<'a> {
    pub fn new(ctx: &'a FactorioContext, type_name: &'a str, item_name: &'a str) -> Self {
        Self {
            ctx,
            type_name,
            item_name,
            quality: 0,
            size: 32.0,
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

    pub fn image(&'_ self) -> egui::Image<'_> {
        let root_path = &self.ctx.icon_path;
        // 某个 type 的 order info 存在，但是没有对应的物品，视为物品不存在
        // 某个 type 的 order info 不存在，当作存在
        let icon_path = if self
            .ctx
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
        let root_path = &self.ctx.icon_path;
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
                            .0
                            .split_top_bottom_at_fraction(0.5)
                            .1,
                        egui::Image::new(format!(
                            "file://{}/{}/{}.png",
                            root_path.to_string_lossy(),
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
    pub ctx: &'a FactorioContext,
    pub item: &'a GenericItem,
    pub size: f32,
}

impl<'a> GenericIcon<'a> {
    pub fn new(ctx: &'a FactorioContext, item: &'a GenericItem) -> Self {
        Self {
            ctx,
            item,
            size: 32.0,
        }
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl<'a> egui::Widget for GenericIcon<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        match self.item {
            GenericItem::Custom { name } => ui.label(format!("特殊: {}", name)),
            GenericItem::Item(IdWithQuality(name, quality)) => ui
                .add_sized(
                    [self.size, self.size],
                    Icon::new(self.ctx, "item", name)
                        .with_quality(*quality)
                        .with_size(self.size),
                )
                .on_hover_text(format!("物品: {}", self.ctx.get_display_name("item", name))),
            GenericItem::Fluid {
                name,
                temperature: _,
            } => ui
                .add_sized(
                    [self.size, self.size],
                    Icon::new(self.ctx, "fluid", name)
                        .with_quality(0)
                        .with_size(self.size),
                )
                .on_hover_text(format!(
                    "流体: {}",
                    self.ctx.get_display_name("fluid", name)
                )),
            GenericItem::Entity(IdWithQuality(name, quality)) => ui
                .add_sized(
                    [self.size, self.size],
                    Icon::new(self.ctx, "entity", name)
                        .with_quality(*quality)
                        .with_size(self.size),
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
