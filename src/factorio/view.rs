use egui::{ScrollArea, Sense, Vec2};

use crate::{
    Subview,
    concept::{AsFlow, GameContextCreatorView},
    factorio::{
        common::{Effect, HasPrototypeBase, OrderInfo},
        format::CompactNumberLabel,
        model::{
            context::{Context, GenericItem},
            mining::MiningConfig,
            recipe::{RecipeConfig, RecipeIngredient, RecipePrototype, RecipeResult},
        },
    },
};

pub struct FactoryView {
    recipe_configs: Vec<Box<dyn AsFlow<ItemIdentType = GenericItem, ContextType = Context>>>,
}

pub struct PlannerView {
    /// 存储游戏逻辑数据的全部上下文
    pub ctx: Context,

    pub factories: Vec<FactoryView>,
    pub selected_factory: usize,

    pub item_selector_storage: ItemSelectorStorage,
}

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
            GenericItem::Fluid { name, temperature } => ui
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
                .add_sized(
                    [self.size, self.size],
                    egui::Label::new(format!("物燃")),
                )
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
            ui.add(CompactNumberLabel::new(self.prototype.energy_required).with_format("{}s"));
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
                                        let icon = ui.add(Icon {
                                            ctx: self.ctx,
                                            type_name: &"item".to_string(),
                                            item_name: &i.name,
                                            size: 32.0,
                                            quality: 0,
                                        });
                                        ui.horizontal_top(|ui| {
                                            ui.vertical(|ui| {
                                                ui.add(CompactNumberLabel::new(i.amount));
                                            });
                                        });
                                    }
                                    RecipeIngredient::Fluid(f) => {
                                        let icon = ui.add(Icon {
                                            ctx: self.ctx,
                                            type_name: &"fluid".to_string(),
                                            item_name: &f.name,
                                            size: 32.0,
                                            quality: 0,
                                        });
                                        ui.vertical(|ui| {
                                            ui.horizontal_top(|ui| {
                                                ui.add(CompactNumberLabel::new(f.amount));
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
                                                                    CompactNumberLabel::new(min_t)
                                                                        .with_format("{}℃"),
                                                                );
                                                                ui.label(" ~ ");
                                                                ui.add(
                                                                    CompactNumberLabel::new(max_t)
                                                                        .with_format("{}℃"),
                                                                );
                                                            });
                                                        }
                                                        (Some(min_t), None) => {
                                                            ui.add(
                                                                CompactNumberLabel::new(min_t)
                                                                    .with_format("≥{}℃"),
                                                            );
                                                        }
                                                        (None, Some(max_t)) => {
                                                            ui.add(
                                                                CompactNumberLabel::new(max_t)
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
                                        let icon = ui.add(Icon {
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

                                                ui.add(CompactNumberLabel::new(
                                                    output.0 - output.1,
                                                ));

                                                ui.add(
                                                    CompactNumberLabel::new(output.1)
                                                        .with_format("+{}"),
                                                );
                                            });
                                        });
                                    }
                                    RecipeResult::Fluid(f) => {
                                        let icon = ui.add(Icon {
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
                                                ui.add(CompactNumberLabel::new(
                                                    output.0 - output.1,
                                                ));
                                                ui.add(
                                                    CompactNumberLabel::new(output.1)
                                                        .with_format("+{}"),
                                                );
                                            });
                                            match f.temperature {
                                                Some(t) => {
                                                    ui.add(
                                                        CompactNumberLabel::new(t)
                                                            .with_format("@{}°C"),
                                                    );
                                                }
                                                None => {
                                                    ui.add(
                                                        CompactNumberLabel::new(
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

#[derive(Debug, Clone, Default)]
pub struct ItemSelectorStorage {
    pub current_type: u8,
    pub group: usize,
    pub subgroup: usize,
    pub index: usize,
    pub selected_item: Option<String>,
}

pub struct ItemSelector<'a> {
    pub ctx: &'a Context,
    pub item_type: &'a String,
    pub order_info: &'a OrderInfo,
    pub storage: &'a mut ItemSelectorStorage,
}

impl egui::Widget for ItemSelector<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let mut response = ui.response().clone();
        let available_space = ui.available_size();
        let group_count = (available_space.x as usize / 70).max(4);
        let item_count = (available_space.x as usize / 35).max(8);
        egui::Grid::new("ItemGroupGrid")
            .min_row_height(64.0)
            .min_col_width(64.0)
            .max_col_width(64.0)
            .spacing(Vec2 { x: 6.0, y: 6.0 })
            .show(ui, |ui| {
                for (i, group) in self.order_info.iter().enumerate() {
                    if (i % group_count) == 0 && i != 0 {
                        ui.end_row();
                    }
                    let group_name = if group.0.is_empty() {
                        "other".to_string()
                    } else {
                        group.0.clone()
                    };
                    if ui
                        .add(Icon {
                            ctx: self.ctx,
                            type_name: &"item-group".to_string(),
                            item_name: &group_name,
                            size: 64.0,
                            quality: 0,
                        })
                        .interact(Sense::click())
                        .clicked()
                    {
                        self.storage.group = i;
                        self.storage.subgroup = 0;
                        self.storage.index = 0;
                        self.storage.selected_item = None;
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
                for (j, subgroup) in self.order_info[self.storage.group].1.iter().enumerate() {
                    for (k, item_name) in subgroup.1.iter().enumerate() {
                        if (k % item_count) == 0 && k != 0 {
                            ui.end_row();
                        }
                        let button = ui
                            .add(Icon {
                                ctx: self.ctx,
                                type_name: self.item_type,
                                item_name,
                                size: 32.0,
                                quality: 0,
                            })
                            .interact(Sense::click());
                        let button = if self.item_type == &"recipe".to_string() {
                            let prototype = self.ctx.recipes.get(item_name).unwrap();
                            button.on_hover_ui_at_pointer(|ui| {
                                ui.add(PrototypeHover {
                                    ctx: self.ctx,
                                    prototype,
                                });
                                ui.label(self.ctx.get_display_name(self.item_type, item_name));
                            })
                        } else {
                            button.on_hover_text_at_pointer(
                                self.ctx.get_display_name(self.item_type, item_name),
                            )
                        };

                        if button.clicked() {
                            self.storage.subgroup = j;
                            self.storage.index = k;
                            self.storage.selected_item = Some(item_name.clone());
                        }
                        if self.storage.subgroup == j && self.storage.index == k {
                            response = response.union(button);
                        }
                    }
                    ui.end_row();
                }
            });
        response
    }
}

impl PlannerView {
    pub fn new(ctx: Context) -> Self {
        let mut ret = PlannerView {
            ctx: ctx.build_order_info(),
            factories: Vec::new(),
            selected_factory: 0,
            item_selector_storage: ItemSelectorStorage::default(),
        };
        ret.factories.push(FactoryView {
            recipe_configs: vec![
                Box::new(RecipeConfig {
                    recipe: "iron-gear-wheel".to_string(),
                    quality: 0,
                    machine: Some("assembling-machine-1".to_string()),
                    modules: vec![],
                    extra_effects: Effect {
                        productivity: 1.0,
                        ..Default::default()
                    },
                    instance_fuel: None,
                }),
                Box::new(MiningConfig {
                    resource: "iron-ore".to_string(),
                    quality: 0,
                    machine: Some("electric-mining-drill".to_string()),
                    modules: vec![],
                    extra_effects: Effect {
                        speed: 1.0,
                        productivity: 2.3,
                        ..Default::default()
                    },
                    instance_fuel: None,
                }),
            ],
        });
        ret
    }
}

impl Default for PlannerView {
    fn default() -> Self {
        Self::new(Context::load(
            &(serde_json::from_str(include_str!("../../assets/data-raw-dump.json"))).unwrap(),
        ))
    }
}

impl Subview for PlannerView {
    fn view(&mut self, ui: &mut egui::Ui) {
        ui.heading("工厂规划器");
        ui.collapsing("模组版本信息", |ui| {
            for (mod_name, mod_version) in &self.ctx.mods {
                ui.label(format!("模组 {} 版本 {}", mod_name, mod_version));
            }
        });
        ui.horizontal(|ui| {
            for i in 0..self.factories.len() {
                if ui
                    .selectable_label(self.selected_factory == i, format!("工厂 {}", i + 1))
                    .clicked()
                {
                    self.selected_factory = i;
                }
            }
        });
        if self.selected_factory >= self.factories.len() {
            ui.label("没有工厂。");
        } else {
            for config in &self.factories[self.selected_factory].recipe_configs {
                let vec_of_map = config.as_flow(&self.ctx);
                let mut keys = vec_of_map[0].keys().collect::<Vec<&GenericItem>>();
                keys.sort_by_key(|g| match g {
                    GenericItem::Item { name, quality } => (
                        *quality as usize,
                        self.ctx
                            .reverse_item_order
                            .as_ref()
                            .unwrap()
                            .get(name)
                            .cloned()
                            .unwrap(),
                        String::new(),
                    ),
                    GenericItem::Fluid { name, temperature } => (
                        0x100usize,
                        self.ctx
                            .reverse_fluid_order
                            .as_ref()
                            .unwrap()
                            .get(name)
                            .cloned()
                            .unwrap(),
                        String::new(),
                    ),
                    GenericItem::Entity { name, quality } => (
                        0x200usize + *quality as usize,
                        self.ctx
                            .reverse_entity_order
                            .as_ref()
                            .unwrap()
                            .get(name)
                            .cloned()
                            .unwrap(),
                        String::new(),
                    ),
                    GenericItem::Heat => (0x300usize, (0usize, 0usize, 0usize), String::new()),
                    GenericItem::Electricity => {
                        (0x400usize, (0usize, 0usize, 0usize), String::new())
                    }
                    GenericItem::FluidHeat { filter } => (
                        0x500usize,
                        (0usize, 0usize, 0usize),
                        filter.clone().unwrap_or_default(),
                    ),
                    GenericItem::FluidFuel { filter } => (
                        0x600usize,
                        (0usize, 0usize, 0usize),
                        filter.clone().unwrap_or_default(),
                    ),
                    GenericItem::ItemFuel { category } => {
                        (0x700usize, (0usize, 0usize, 0usize), category.clone())
                    }
                    GenericItem::RocketPayloadWeight => {
                        (0x800usize, (0usize, 0usize, 0usize), String::new())
                    }
                    GenericItem::RocketPayloadStack => {
                        (0x900usize, (0usize, 0usize, 0usize), String::new())
                    }
                    GenericItem::Pollution { name } => {
                        (0xa00usize, (0usize, 0usize, 0usize), name.clone())
                    }
                    GenericItem::Custom { name } => {
                        (0xb00usize, (0usize, 0usize, 0usize), name.clone())
                    }
                });

                ui.horizontal_top(|ui| {
                    for key in keys {
                        let amount = vec_of_map[0].get(key).unwrap();

                        ui.vertical(|ui| {
                            ui.add_sized(
                                [35.0, 35.0],
                                GenericIcon {
                                    ctx: &self.ctx,
                                    item: key,
                                    size: 32.0,
                                },
                            );
                            ui.add_sized(
                                [35.0, 10.0],
                                CompactNumberLabel::new(*amount).with_format("{}"),
                            );
                        });
                    }
                });
            }
        }
        ScrollArea::new([false, true])
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(ItemSelector {
                    ctx: &self.ctx,
                    item_type: &"recipe".to_string(),
                    order_info: self.ctx.recipe_order.as_ref().unwrap(),
                    storage: &mut self.item_selector_storage,
                });
                // for group in self.ctx.item_order.as_ref().unwrap().iter() {
                //     ui.collapsing(format!("Group {}", group.0), |ui| {
                //         for subgroup in group.1.iter() {
                //             ui.collapsing(format!("Subgroup {}", subgroup.0), |ui| {
                //                 for item_name in subgroup.1.iter() {
                //                     ui.label(item_name);
                //                     if let Some(item) = self.ctx.items.get(item_name) {
                //                         if let Some(icon_path) = &self.ctx.icon_path {
                //                             ui.add(Icon {
                //                                 ctx: &self.ctx,
                //                                 type_name: &"item".to_string(),
                //                                 item_name,
                //                                 size: 32.0,
                //                                 quality: 0,
                //                             });
                //                         } else {
                //                             ui.label("未找到图标路径！");
                //                         }
                //                         ui.label(format!("物品: {}", item_name));
                //                         ui.label(format!("{:#?}", item));
                //                     } else {
                //                         ui.label("未找到该物品！");
                //                     }
                //                 }
                //             });
                //         }
                //     });
                // }
                // for group in self.ctx.recipe_order.as_ref().unwrap().iter() {
                //     ui.collapsing(format!("Recipe Group {}", group.0), |ui| {
                //         for subgroup in group.1.iter() {
                //             ui.collapsing(format!("Recipe Subgroup {}", subgroup.0), |ui| {
                //                 for recipe_name in subgroup.1.iter() {
                //                     ui.collapsing(format!("Recipe {}", recipe_name), |ui| {
                //                         if let Some(recipe) = self.ctx.recipes.get(recipe_name) {
                //                             if let Some(icon_path) = &self.ctx.icon_path {
                //                                 ui.add(Icon {
                //                                     ctx: &self.ctx,
                //                                     type_name: &"recipe".to_string(),
                //                                     item_name: recipe_name,
                //                                     size: 32.0,
                //                                     quality: 0,
                //                                 });
                //                             } else {
                //                                 ui.label("未找到图标路径！");
                //                             }
                //                             ui.label(format!("配方: {}", recipe_name));
                //                             ui.label(format!("{:#?}", recipe));
                //                         } else {
                //                             ui.label("未找到该配方！");
                //                         }
                //                     });
                //                 }
                //             });
                //         }
                //     });
                // }
            });
    }
}

#[derive(Default, Debug)]
pub struct FactorioContextCreatorView {
    path: Option<std::path::PathBuf>,
    mod_path: Option<std::path::PathBuf>,
    subview_sender: Option<std::sync::mpsc::Sender<Box<dyn Subview>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Subview for FactorioContextCreatorView {
    fn view(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("Context Creator");
            ui.separator();

            ui.label("选择游戏路径:");
            if ui.button("浏览...").clicked()
                && let Some(path) = rfd::FileDialog::new().pick_file()
            {
                self.path = Some(path);
            }
            if let Some(path) = &self.path {
                ui.label(format!("已选择路径: {}", path.display()));
            } else {
                ui.label("未选择路径");
            }

            ui.separator();

            ui.label("选择Mod路径 (可选):");
            if ui.button("浏览...").clicked() {
                if let Some(mod_path) = rfd::FileDialog::new().pick_folder() {
                    self.mod_path = Some(mod_path);
                } else {
                    self.mod_path = None;
                }
            }

            if let Some(mod_path) = &self.mod_path {
                ui.label(format!("已选择Mod路径: {}", mod_path.display()));
            } else {
                ui.label("未选择Mod路径");
            }

            ui.separator();

            if ui
                .add_enabled(
                    self.path.is_some() && self.thread.is_none(),
                    egui::Button::new("加载游戏上下文"),
                )
                .clicked()
                && let Some(path) = &self.path
                && let Some(sender) = &self.subview_sender
                && let None = self.thread
            {
                let exe_path = path.clone().as_path().to_owned();
                let mod_path = self.mod_path.clone().map(|p| p.as_path().to_owned());
                let sender = sender.clone();
                self.thread = Some(std::thread::spawn(move || {
                    if let Some(ctx) =
                        Context::load_from_executable_path(&exe_path, mod_path.as_deref(), None)
                    {
                        sender
                            .send(Box::new(PlannerView::new(ctx)))
                            .expect("Failed to send subview");
                    }
                }));
            }

            ui.separator();

            if ui
                .add_enabled(self.thread.is_none(), egui::Button::new("加载缓存上下文"))
                .clicked()
                && let Some(sender) = &self.subview_sender
                && let None = self.thread
            {
                let sender = sender.clone();
                self.thread = Some(std::thread::spawn(move || {
                    if let Some(ctx) = Context::load_from_tmp_no_dump() {
                        sender
                            .send(Box::new(PlannerView::new(ctx)))
                            .expect("Failed to send subview");
                    }
                }));
            }
            if let Some(ref thread) = self.thread
                && thread.is_finished()
            {
                let thread = self.thread.take().unwrap();
                thread.join().expect("Failed to join thread");
            }
        });
    }
}

impl GameContextCreatorView for FactorioContextCreatorView {
    fn set_subview_sender(&mut self, sender: std::sync::mpsc::Sender<Box<dyn Subview>>) {
        self.subview_sender = Some(sender);
    }
    // fn try_create_subview(&mut self) -> Option<Box<dyn SubView>> {
    //     if self.created_context.is_some() {
    //         return Some(Box::new(PlannerView {
    //             ctx: self.created_context.take().unwrap(),
    //             factories: vec![FactoryView {
    //                 recipe_configs: vec![
    //                     Box::new(RecipeConfig {
    //                         recipe: "iron-gear-wheel".to_string(),
    //                         machine: Some("assembling-machine-2".to_string()),
    //                         modules: vec![
    //                             ("speed-module".to_string(), 0),
    //                             ("speed-module".to_string(), 0),
    //                         ],
    //                         quality: 0,
    //                         extra_effects: Effect::default(),
    //                     }),
    //                     Box::new(RecipeConfig {
    //                         recipe: "iron-plate".to_string(),
    //                         machine: Some("stone-furnace".to_string()),
    //                         modules: vec![("productivity-module-3".to_string(), 0); 5],
    //                         quality: 0,
    //                         extra_effects: Effect::default(),
    //                     }),
    //                     Box::new(RecipeConfig {
    //                         recipe: "copper-plate".to_string(),
    //                         machine: Some("stone-furnace".to_string()),
    //                         modules: vec![("productivity-module-3".to_string(), 0); 2],
    //                         quality: 0,
    //                         extra_effects: Effect::default(),
    //                     }),
    //                     Box::new(RecipeConfig {
    //                         recipe: "copper-cable".to_string(),
    //                         machine: Some("assembling-machine-2".to_string()),
    //                         modules: vec![],
    //                         quality: 0,
    //                         extra_effects: Effect {
    //                             speed: 100.0,
    //                             ..Default::default()
    //                         },
    //                     }),
    //                     Box::new(RecipeConfig {
    //                         recipe: "electronic-circuit".to_string(),
    //                         machine: Some("assembling-machine-2".to_string()),
    //                         modules: vec![],
    //                         quality: 0,
    //                         extra_effects: Effect::default(),
    //                     }),
    //                     Box::new(MiningConfig {
    //                         resource: "iron-ore".to_string(),
    //                         quality: 0,
    //                         machine: Some("big-mining-drill".to_string()),
    //                         modules: vec![],
    //                         extra_effects: Effect::default(),
    //                     }),
    //                 ],
    //             }],
    //             selected_factory: 0,
    //             item_selector_storage: ItemSelectorStorage::default(),
    //         }));
    //     }
    //     None
    // }
}
