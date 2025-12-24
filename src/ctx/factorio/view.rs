use std::any::Any;

use egui::{Image, ScrollArea, Sense, Vec2};

use crate::{
    SubView,
    ctx::{
        GameContextCreatorView, RecipeLike,
        factorio::{
            common::{Effect, OrderInfo},
            context::{Context, GenericItem},
            mining::MiningConfig,
            recipe::RecipeConfig,
        },
    },
};

pub(crate) trait ConfigView {
    fn ui(&self, ui: &mut egui::Ui, ctx: &Context);
}

impl<T: RecipeLike<ContextType = Context> + 'static> ConfigView for T {
    fn ui(&self, ui: &mut egui::Ui, ctx: &Context) {
        ui.label(format!(
            "配方类型:{:?}\n配方转化: {:?}",
            self.type_id(),
            self.as_hash_map(ctx)
        ));
    }
}
pub(crate) struct FactoryView {
    recipe_configs: Vec<Box<dyn ConfigView>>,
}

pub(crate) struct PlannerView {
    /// 存储游戏逻辑数据的全部上下文
    pub(crate) ctx: Context,

    pub(crate) factories: Vec<FactoryView>,
    pub(crate) selected_factory: usize,

    pub(crate) item_selector_storage: ItemSelectorStorage,
}

#[derive(Debug, Clone)]

pub(crate) struct Icon<'a> {
    pub(crate) root_path: &'a std::path::Path,
    pub(crate) type_name: &'a String,
    pub(crate) item_name: &'a String,
    pub(crate) size: f32,
}

impl<'a> Icon<'a> {
    fn image(&'_ self) -> egui::Image<'_> {
        let icon_path = format!(
            "file://{}/{}/{}.png",
            self.root_path.to_string_lossy(),
            self.type_name,
            self.item_name
        );
        egui::Image::new(icon_path)
    }
}

impl<'a> egui::Widget for Icon<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        ui.add(
            self.image()
                .fit_to_exact_size(Vec2 {
                    x: self.size,
                    y: self.size,
                })
                .show_loading_spinner(true)
                .maintain_aspect_ratio(true)
                .bg_fill(egui::Color32::from_rgba_premultiplied(
                    0xaa, 0xaa, 0xaa, 0xcc,
                ))
                .corner_radius(4.0),
        )
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
    pub(crate) icon_path: &'a std::path::Path,
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
                            root_path: self.icon_path,
                            type_name: &"item-group".to_string(),
                            item_name: &group_name,
                            size: 64.0,
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
                            .add(
                                Icon {
                                    root_path: self.icon_path,
                                    type_name: self.item_type,
                                    item_name,
                                    size: 32.0,
                                },
                            )
                            .interact(Sense::click());

                        if button.clicked() {
                            self.storage.subgroup = j;
                            self.storage.index = k;
                            self.storage.selected_item = Some(item_name.clone());
                        }
                        if self.storage.group == self.storage.group
                            && self.storage.subgroup == j
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
                config.ui(ui, &self.ctx);
            }
        }
        ScrollArea::new([false, true])
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(ItemSelector {
                    icon_path: self.ctx.icon_path.as_ref().unwrap(),
                    item_type: &"item".to_string(),
                    order_info: self.ctx.item_order.as_ref().unwrap(),
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
                                                root_path: icon_path.as_path(),
                                                type_name: &"item".to_string(),
                                                item_name: item_name,
                                                size: 32.0,
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
                                                    root_path: icon_path.as_path(),
                                                    type_name: &"recipe".to_string(),
                                                    item_name: recipe_name,
                                                    size: 32.0,
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
            if ui.button("浏览...").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    self.path = Some(path);
                }
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

            if ui.button("加载上下文").clicked() {
                if let Some(path) = &self.path {
                    self.created_context = Context::load_from_executable_path(
                        path,
                        self.mod_path.as_deref(),
                        Some("zh-CN"),
                    );
                }
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
