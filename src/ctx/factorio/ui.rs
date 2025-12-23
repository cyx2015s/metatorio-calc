use std::{
    any::{Any, TypeId},
    fs,
};

use egui::{AtomExt, ImageSource, ScrollArea, Vec2};

use crate::{
    SubView,
    ctx::{
        GameContextCreatorView, RecipeLike,
        factorio::{
            common::{Effect, OrderInfo, ReverseOrderInfo},
            context::FactorioContext,
            mining::MiningConfig,
            recipe::RecipeConfig,
        },
    },
};

pub(crate) trait ConfigView {
    fn ui(&self, ui: &mut egui::Ui, ctx: &FactorioContext);
}

impl<T: RecipeLike<ContextType = FactorioContext> + 'static> ConfigView for T {
    fn ui(&self, ui: &mut egui::Ui, ctx: &FactorioContext) {
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

pub(crate) struct FactorioPlanner {
    /// 存储游戏逻辑数据的全部上下文
    pub(crate) ctx: FactorioContext,

    /// 物品遍历顺序，按大组、按小组、按自身
    pub(crate) item_order: Option<OrderInfo>,
    pub(crate) reverse_item_order: Option<ReverseOrderInfo>,

    /// 配方遍历顺序，按大组、按小组、按自身
    pub(crate) recipe_order: Option<OrderInfo>,
    pub(crate) reverse_recipe_order: Option<ReverseOrderInfo>,

    pub(crate) factories: Vec<FactoryView>,
    pub(crate) selected_factory: usize,
}

impl FactorioPlanner {
    pub(crate) fn new(ctx: FactorioContext) -> Self {
        FactorioPlanner {
            item_order: None,
            ctx,
            factories: Vec::new(),
            selected_factory: 0,
            reverse_item_order: None,
            recipe_order: None,
            reverse_recipe_order: None,
        }
    }
}

impl Default for FactorioPlanner {
    fn default() -> Self {
        Self::new(FactorioContext::load(
            &(serde_json::from_str(include_str!("../../../assets/data-raw-dump.json"))).unwrap(),
        ))
    }
}

impl SubView for FactorioPlanner {
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
        if self.item_order.is_none() {
            self.item_order = Some(crate::ctx::factorio::common::get_order_info(
                &self.ctx.items,
                &self.ctx.groups,
                &self.ctx.subgroups,
            ));
            self.reverse_item_order = Some(crate::ctx::factorio::common::get_reverse_order_info(
                self.item_order.as_ref().unwrap(),
            ));
        }
        if self.recipe_order.is_none() {
            self.recipe_order = Some(crate::ctx::factorio::common::get_order_info(
                &self.ctx.recipes,
                &self.ctx.groups,
                &self.ctx.subgroups,
            ));
            self.reverse_recipe_order = Some(crate::ctx::factorio::common::get_reverse_order_info(
                self.recipe_order.as_ref().unwrap(),
            ));
        }
        ScrollArea::new([false, true])
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for group in self.item_order.as_ref().unwrap().iter() {
                    ui.collapsing(format!("Group {}", group.0), |ui| {
                        for subgroup in group.1.iter() {
                            ui.collapsing(format!("Subgroup {}", subgroup.0), |ui| {
                                for item_name in subgroup.1.iter() {
                                    ui.label(item_name);
                                }
                            });
                        }
                    });
                }
                for group in self.recipe_order.as_ref().unwrap().iter() {
                    ui.collapsing(format!("Recipe Group {}", group.0), |ui| {
                        for subgroup in group.1.iter() {
                            ui.collapsing(format!("Recipe Subgroup {}", subgroup.0), |ui| {
                                for recipe_name in subgroup.1.iter() {
                                    ui.collapsing(format!("Recipe {}", recipe_name), |ui| {
                                        if let Some(recipe) = self.ctx.recipes.get(recipe_name) {
                                            if let Some(icon_path) = &self.ctx.icon_path {
                                                let recipe_icon_path = format!(
                                                    "file:///{}/recipe/{}.png",
                                                    icon_path.to_string_lossy(),
                                                    recipe_name
                                                );
                                                ui.add(
                                                    egui::Image::new(recipe_icon_path)
                                                        .fit_to_exact_size(Vec2 {
                                                            x: 128.0,
                                                            y: 128.0,
                                                        }).show_loading_spinner(true).maintain_aspect_ratio(true).bg_fill(egui::Color32::from_rgb(11, 45, 14)),
                                                );
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
pub(crate) struct FactorioContextCreatorView {
    path: Option<std::path::PathBuf>,
    skip_dumping: bool,
    created_context: Option<FactorioContext>,
}

impl SubView for FactorioContextCreatorView {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("加载异星工厂上下文");
        if let Some(path) = &self.path {
            ui.label(format!("当前路径: {}", path.display()));
        } else {
            ui.label("当前路径: 未选择");
        }
        ui.button("选择路径")
            .on_hover_text("选择异星工厂的可执行文件")
            .clicked()
            .then(|| {
                if let Some(selected_path) = rfd::FileDialog::new().pick_file() {
                    self.path = Some(selected_path);
                }
            });
        ui.checkbox(&mut self.skip_dumping, "跳过数据转储");
        if self.path.is_some() {
            if ui
                .button("新建上下文（阻塞！！！）")
                .on_hover_text("从所选路径加载异星工厂上下文数据")
                .clicked()
            {
                if self.skip_dumping {
                    self.created_context = FactorioContext::load_from_tmp_no_dump();
                } else if let Some(ctx) =
                    FactorioContext::load_from_executable_path(self.path.as_ref().unwrap())
                {
                    self.created_context = Some(ctx);
                }
            }
        }
    }
}

impl GameContextCreatorView for FactorioContextCreatorView {
    fn try_create_subview(&mut self) -> Option<Box<dyn SubView>> {
        if self.created_context.is_some() {
            return Some(Box::new(FactorioPlanner {
                item_order: None,
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
                reverse_item_order: None,
                recipe_order: None,
                reverse_recipe_order: None,
            }));
        }
        None
    }
}
