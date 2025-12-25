
use egui::{ScrollArea, Sense, Vec2};

use crate::{
    SubView,
    ctx::{
        GameContextCreatorView, RecipeLike,
        factorio::{
            common::{Effect, HasPrototypeBase, OrderInfo},
            context::{Context, GenericItem},
            mining::MiningConfig,
            recipe::{RecipeConfig, RecipeIngredient, RecipePrototype, RecipeResult},
        },
    },
};

pub(crate) struct FactoryView {
    recipe_configs: Vec<Box<dyn RecipeLike<KeyType = GenericItem, ContextType = Context>>>,
}

pub(crate) struct PlannerView {
    /// 存储游戏逻辑数据的全部上下文
    pub(crate) ctx: Context,

    pub(crate) factories: Vec<FactoryView>,
    pub(crate) selected_factory: usize,

    pub(crate) item_selector_storage: ItemSelectorStorage,
}

#[derive(Debug)]

pub(crate) struct Icon<'a> {
    pub(crate) ctx: &'a Context,
    pub(crate) type_name: &'a String,
    pub(crate) item_name: &'a String,
    pub(crate) quality: u8,
    pub(crate) size: f32,
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
pub(crate) struct GenericIcon<'a> {
    pub(crate) ctx: &'a Context,
    pub(crate) item: &'a GenericItem,
    pub(crate) size: f32,
}

impl<'a> egui::Widget for GenericIcon<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        match self.item {
            GenericItem::Custom { name } => ui.label(format!("Custom Item: {}", name)),
            GenericItem::Item { name, quality } => {
                let icon = ui.add(Icon {
                    ctx: self.ctx,
                    type_name: &"item".to_string(),
                    item_name: name,
                    size: self.size,
                    quality: 0,
                });
                ui.put(
                    icon.rect
                        .split_left_right_at_fraction(0.5)
                        .1
                        .split_left_right_at_fraction(0.5)
                        .1,
                    Icon {
                        ctx: self.ctx,
                        type_name: &"quality".to_string(),
                        item_name: &format!("{}", quality),
                        size: self.size / 2.0,
                        quality: *quality,
                    },
                );
                icon
            }
            GenericItem::Fluid { name, temperature } => {
                
                ui.add(Icon {
                    ctx: self.ctx,
                    type_name: &"fluid".to_string(),
                    item_name: name,
                    size: self.size,
                    quality: 0,
                })
            }
            GenericItem::Entity { name, quality } => {
                
                ui.add(Icon {
                    ctx: self.ctx,
                    type_name: &"entity".to_string(),
                    item_name: name,
                    size: self.size,
                    quality: *quality,
                })
            }
            GenericItem::Heat => ui.label("热量"),
            GenericItem::Electricity => ui.label("电力"),
            GenericItem::FluidHeat => ui.label("流体热量"),
            GenericItem::FluidFuel => ui.label("流体燃料"),
            GenericItem::ItemFuel { category } => ui.label(format!("燃料: {}", category)),
            GenericItem::RocketPayloadWeight => ui.label("火箭重量载荷"),
            GenericItem::RocketPayloadStack => ui.label("火箭堆叠载荷"),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PrototypeDetailView<'a, T: HasPrototypeBase> {
    pub(crate) ctx: &'a Context,
    pub(crate) prototype: &'a T,
}

impl<'a> egui::Widget for PrototypeDetailView<'a, RecipePrototype> {
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
            ui.label(format!("{}s", self.prototype.energy_required));
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
                                        ui.vertical(|ui| {
                                            ui.label(format!("x{}", i.amount));
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
                                            ui.label(format!("x{}", f.amount));
                                            match f.temperature {
                                                Some(t) => {
                                                    ui.label(format!("{}℃", t));
                                                }
                                                None => {
                                                    match (f.min_temperature, f.max_temperature) {
                                                        (Some(min_t), Some(max_t)) => {
                                                            ui.label(format!(
                                                                "{}~{}℃",
                                                                min_t, max_t
                                                            ));
                                                        }
                                                        (Some(min_t), None) => {
                                                            ui.label(format!("≥{}℃", min_t));
                                                        }
                                                        (None, Some(max_t)) => {
                                                            ui.label(format!("≤{}℃", max_t));
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
                                            ui.label(format!(
                                                "x{}+{}",
                                                output.0 - output.1,
                                                output.1
                                            ));
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
                                            ui.label(format!(
                                                "x{}+{}",
                                                output.0 - output.1,
                                                output.1
                                            ));
                                            match f.temperature {
                                                Some(t) => {
                                                    ui.label(format!("@{}°C", t));
                                                }
                                                None => {
                                                    ui.label(format!(
                                                        "@{}°C",
                                                        &self
                                                            .ctx
                                                            .fluids
                                                            .get(&f.name)
                                                            .unwrap()
                                                            .default_temperature
                                                    ));
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
pub(crate) struct ItemSelectorStorage {
    pub(crate) current_type: u8,
    pub(crate) group: usize,
    pub(crate) subgroup: usize,
    pub(crate) index: usize,
    pub(crate) selected_item: Option<String>,
}

pub(crate) struct ItemSelector<'a> {
    pub(crate) ctx: &'a Context,
    pub(crate) item_type: &'a String,
    pub(crate) order_info: &'a OrderInfo,
    pub(crate) storage: &'a mut ItemSelectorStorage,
}

impl egui::Widget for ItemSelector<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let mut response = ui.response().clone();
        egui::Grid::new("ItemGroupGrid")
            .min_row_height(64.0)
            .min_col_width(64.0)
            .max_col_width(64.0)
            .spacing(Vec2 { x: 6.0, y: 6.0 })
            .show(ui, |ui| {
                for (i, group) in self.order_info.iter().enumerate() {
                    if (i % 8) == 0 && i != 0 {
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
            .num_columns(16)
            .max_col_width(35.0)
            .min_col_width(35.0)
            .min_row_height(35.0)
            .spacing(Vec2 { x: 0.0, y: 0.0 })
            .striped(true)
            .show(ui, |ui| {
                for (j, subgroup) in self.order_info[self.storage.group].1.iter().enumerate() {
                    for (k, item_name) in subgroup.1.iter().enumerate() {
                        if (k % 16) == 0 && k != 0 {
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
                                ui.add(PrototypeDetailView {
                                    ctx: self.ctx,
                                    prototype,
                                });
                            })
                        } else {
                            button
                        };

                        if button.clicked() {
                            self.storage.subgroup = j;
                            self.storage.index = k;
                            self.storage.selected_item = Some(item_name.clone());
                        }
                        if self.storage.subgroup == j
                            && self.storage.index == k
                        {
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
    pub(crate) fn new(ctx: Context) -> Self {
        PlannerView {
            ctx: ctx.build_order_info(),
            factories: Vec::new(),
            selected_factory: 0,
            item_selector_storage: ItemSelectorStorage::default(),
        }
    }
}

impl Default for PlannerView {
    fn default() -> Self {
        Self::new(Context::load(
            &(serde_json::from_str(include_str!("../../../assets/data-raw-dump.json"))).unwrap(),
        ))
    }
}

impl SubView for PlannerView {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Factorio Planner");

        ui.horizontal(|ui| {
            for i in 0..self.factories.len() {
                if ui
                    .selectable_label(self.selected_factory == i, format!("Factory {}", i + 1))
                    .clicked()
                {
                    self.selected_factory = i;
                }
            }
        });
        if self.selected_factory >= self.factories.len() {
            ui.label("没有工厂。");
        } else {
            for config in self.factories[self.selected_factory].recipe_configs.iter() {
                ui.label(format!("{:?}", config.as_hash_map(&self.ctx)));
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
                for group in self.ctx.item_order.as_ref().unwrap().iter() {
                    ui.collapsing(format!("Group {}", group.0), |ui| {
                        for subgroup in group.1.iter() {
                            ui.collapsing(format!("Subgroup {}", subgroup.0), |ui| {
                                for item_name in subgroup.1.iter() {
                                    ui.label(item_name);
                                    if let Some(item) = self.ctx.items.get(item_name) {
                                        if let Some(icon_path) = &self.ctx.icon_path {
                                            ui.add(Icon {
                                                ctx: &self.ctx,
                                                type_name: &"item".to_string(),
                                                item_name,
                                                size: 32.0,
                                                quality: 0,
                                            });
                                        } else {
                                            ui.label("未找到图标路径！");
                                        }
                                        ui.label(format!("物品: {}", item_name));
                                        ui.label(format!("{:#?}", item));
                                    } else {
                                        ui.label("未找到该物品！");
                                    }
                                }
                            });
                        }
                    });
                }
                for group in self.ctx.recipe_order.as_ref().unwrap().iter() {
                    ui.collapsing(format!("Recipe Group {}", group.0), |ui| {
                        for subgroup in group.1.iter() {
                            ui.collapsing(format!("Recipe Subgroup {}", subgroup.0), |ui| {
                                for recipe_name in subgroup.1.iter() {
                                    ui.collapsing(format!("Recipe {}", recipe_name), |ui| {
                                        if let Some(recipe) = self.ctx.recipes.get(recipe_name) {
                                            if let Some(icon_path) = &self.ctx.icon_path {
                                                ui.add(Icon {
                                                    ctx: &self.ctx,
                                                    type_name: &"recipe".to_string(),
                                                    item_name: recipe_name,
                                                    size: 32.0,
                                                    quality: 0,
                                                });
                                            } else {
                                                ui.label("未找到图标路径！");
                                            }
                                            ui.label(format!("配方: {}", recipe_name));
                                            ui.label(format!("{:#?}", recipe));
                                        } else {
                                            ui.label("未找到该配方！");
                                        }
                                    });
                                }
                            });
                        }
                    });
                }
            });
    }
}

#[derive(Default, Debug)]
pub(crate) struct ContextCreatorView {
    path: Option<std::path::PathBuf>,
    mod_path: Option<std::path::PathBuf>,
    created_context: Option<Context>,
}

impl SubView for ContextCreatorView {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("Context Creator");
            ui.separator();

            ui.label("选择游戏路径:");
            if ui.button("浏览...").clicked()
                && let Some(path) = rfd::FileDialog::new().pick_file() {
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

            if ui.button("加载上下文").clicked()
                && let Some(path) = &self.path {
                    self.created_context = Context::load_from_executable_path(
                        path,
                        self.mod_path.as_deref(),
                        Some("zh-CN"),
                    );
                }

            ui.separator();

            if ui.button("加载缓存上下文").clicked() {
                self.created_context = Context::load_from_tmp_no_dump();
            }
        });
    }
}

impl GameContextCreatorView for ContextCreatorView {
    fn try_create_subview(&mut self) -> Option<Box<dyn SubView>> {
        if self.created_context.is_some() {
            return Some(Box::new(PlannerView {
                ctx: self.created_context.take().unwrap(),
                factories: vec![FactoryView {
                    recipe_configs: vec![
                        Box::new(RecipeConfig {
                            recipe: "iron-gear-wheel".to_string(),
                            machine: Some("assembling-machine-2".to_string()),
                            modules: vec![],
                            quality: 0,
                            extra_effects: Effect::default(),
                        }),
                        Box::new(RecipeConfig {
                            recipe: "iron-plate".to_string(),
                            machine: Some("stone-furnace".to_string()),
                            modules: vec![],
                            quality: 0,
                            extra_effects: Effect::default(),
                        }),
                        Box::new(RecipeConfig {
                            recipe: "copper-plate".to_string(),
                            machine: Some("stone-furnace".to_string()),
                            modules: vec![],
                            quality: 0,
                            extra_effects: Effect::default(),
                        }),
                        Box::new(RecipeConfig {
                            recipe: "copper-cable".to_string(),
                            machine: Some("assembling-machine-2".to_string()),
                            modules: vec![],
                            quality: 0,
                            extra_effects: Effect::default(),
                        }),
                        Box::new(RecipeConfig {
                            recipe: "electronic-circuit".to_string(),
                            machine: Some("assembling-machine-2".to_string()),
                            modules: vec![],
                            quality: 0,
                            extra_effects: Effect::default(),
                        }),
                        Box::new(MiningConfig {
                            resource: "iron-ore".to_string(),
                            quality: 0,
                            machine: Some("big-mining-drill".to_string()),
                            modules: vec![],
                            extra_effects: Effect::default(),
                        }),
                    ],
                }],
                selected_factory: 0,
                item_selector_storage: ItemSelectorStorage::default(),
            }));
        }
        None
    }
}
