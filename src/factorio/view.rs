use egui::{ScrollArea, Sense, Vec2};

use crate::{
    Subview,
    concept::{AsFlowEditor, GameContextCreatorView},
    factorio::{
        common::{Effect, HasPrototypeBase, OrderInfo},
        format::{CompactLabel, SignedCompactLabel},
        model::{
            context::{Context, GenericItem},
            module::ModuleConfig,
            recipe::{RecipeConfig, RecipeIngredient, RecipePrototype, RecipeResult},
        },
    },
};

pub struct FactoryInstance {
    name: String,
    recipe_configs: Vec<Box<dyn AsFlowEditor<ItemIdentType = GenericItem, ContextType = Context>>>,
}

pub struct PlannerView {
    /// 存储游戏逻辑数据的全部上下文
    pub ctx: Context,

    pub factories: Vec<FactoryInstance>,
    pub selected_factory: usize,
}

impl FactoryInstance {
    fn view(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &Context,
    ) {
        ui.heading(&self.name);
        ScrollArea::vertical().show(ui, |ui| {
            for recipe_config in &mut self.recipe_configs {
                recipe_config.editor_view(ui, ctx);
                ui.separator();
            }
        });
    }
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
    pub selected_item: &'a mut Option<String>,
}

impl egui::Widget for ItemSelector<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let mut response = ui.response().clone();
        let available_space = ui.available_size();
        let group_count = (available_space.x as usize / 70).max(4);
        let item_count = (available_space.x as usize / 35).max(8);
        let id = ui.id();
        let mut storage: ItemSelectorStorage = ui.memory(move |mem| {
            mem.data
                .get_temp::<ItemSelectorStorage>(id)
                .unwrap_or_default()
        });

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
                        storage.group = i;
                        storage.subgroup = 0;
                        storage.index = 0;
                        storage.selected_item = None;
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
                for (j, subgroup) in self.order_info[storage.group].1.iter().enumerate() {
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
                            button.on_hover_ui(|ui| {
                                ui.add(PrototypeHover {
                                    ctx: self.ctx,
                                    prototype,
                                });
                            })
                        } else {
                            button
                                .on_hover_text(self.ctx.get_display_name(self.item_type, item_name))
                        };

                        if button.clicked() {
                            storage.subgroup = j;
                            storage.index = k;
                            storage.selected_item = Some(item_name.clone());
                            self.selected_item.replace(item_name.clone());
                        }
                        if storage.subgroup == j && storage.index == k {
                            response = response.union(button);
                        }
                    }
                    ui.end_row();
                }
            });
        ui.memory_mut(move |mem| {
            mem.data
                .insert_temp::<ItemSelectorStorage>(id, storage.clone());
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
        };
        ret.factories.push(FactoryInstance {
            name: "工厂".to_string(),
            recipe_configs: vec![
                Box::new(RecipeConfig {
                    recipe: ("iron-gear-wheel".to_string()).into(),
                    machine: Some(("assembling-machine-1".to_string()).into()),
                    module_config: ModuleConfig::default(),
                    extra_effects: Effect::default(),
                    instance_fuel: None,
                }),
                Box::new(RecipeConfig {
                    recipe: ("copper-cable".into()),

                    machine: Some("assembling-machine-2".into()),
                    module_config: ModuleConfig::default(),
                    extra_effects: Effect::default(),
                    instance_fuel: None,
                }),
                Box::new(RecipeConfig {
                    recipe: ("transport-belt".into()),
                    machine: Some("assembling-machine-2".into()),
                    module_config: ModuleConfig::default(),
                    extra_effects: Effect::default(),
                    instance_fuel: None,
                }),
                // Box::new(MiningConfig {
                //     resource: "iron-ore".to_string(),
                //     quality: 0,
                //     machine: Some(("electric-mining-drill".to_string(), 0)),
                //     modules: vec![],
                //     extra_effects: Effect {
                //         speed: 1.0,
                //         productivity: 2.3,
                //         ..Default::default()
                //     },
                //     instance_fuel: None,
                // }),
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
            self.factories[self.selected_factory].view(ui, &self.ctx);
        }
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
}
