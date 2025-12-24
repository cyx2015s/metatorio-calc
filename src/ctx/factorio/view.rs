use std::any::Any;

use egui::{ScrollArea, Vec2};

use crate::{
    SubView,
    ctx::{
        GameContextCreatorView, RecipeLike,
        factorio::{
            common::{Effect, OrderInfo, ReverseOrderInfo},
            context::Context,
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
}

#[derive(Debug, Clone, Default)]

pub(crate) struct Icon {
    pub(crate) root_path: std::path::PathBuf,
    pub(crate) type_name: String,
    pub(crate) item_name: String,
    pub(crate) size: f32,
}

impl egui::Widget for Icon {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let icon_path = format!(
            "file://{}/{}/{}.png",
            self.root_path.to_string_lossy(),
            self.type_name,
            self.item_name
        );
        ui.add(
            egui::Image::new(icon_path)
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

impl PlannerView {
    pub(crate) fn new(ctx: Context) -> Self {
        PlannerView {
            ctx: ctx.build_order_info(),
            factories: Vec::new(),
            selected_factory: 0,
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
                for group in self.ctx.item_order.as_ref().unwrap().iter() {
                    ui.collapsing(format!("Group {}", group.0), |ui| {
                        for subgroup in group.1.iter() {
                            ui.collapsing(format!("Subgroup {}", subgroup.0), |ui| {
                                for item_name in subgroup.1.iter() {
                                    ui.label(item_name);
                                    if let Some(item) = self.ctx.items.get(item_name) {
                                        if let Some(icon_path) = &self.ctx.icon_path {
                                            ui.add(Icon {
                                                root_path: icon_path.clone(),
                                                type_name: "item".to_string(),
                                                item_name: item_name.clone(),
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
                                                    root_path: icon_path.clone(),
                                                    type_name: "recipe".to_string(),
                                                    item_name: recipe_name.clone(),
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
            }));
        }
        None
    }
}
