use egui::ScrollArea;

use crate::{
    Renderable,
    context::{GameContextCreator, RecipeLike},
    ctx::factorio::{
        common::{OrderInfo, ReverseOrderInfo},
        context::FactorioContext,
        recipe::RecipeConfig,
    },
};

pub(crate) trait ConfigView {
    fn ui(&self, ui: &mut egui::Ui, ctx: &FactorioContext);
}

impl ConfigView for RecipeConfig {
    fn ui(&self, ui: &mut egui::Ui, ctx: &FactorioContext) {
        ui.horizontal(|ui| {
            ui.label("配方");
            let hash_map = self.as_hash_map(&ctx);
            for (item, amount) in hash_map.iter() {
                ui.label(format!("{:?}: {}", item, amount));
            }
        });
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
        let mut ret = FactorioPlanner {
            item_order: None,
            ctx,
            factories: Vec::new(),
            selected_factory: 0,
            reverse_item_order: None,
            recipe_order: None,
            reverse_recipe_order: None,
        };
        ret.factories.push(FactoryView {
            recipe_configs: vec![
                Box::new(RecipeConfig {
                    recipe: "iron-gear-wheel".to_string(),
                    machine: Some("assembling-machine-2".to_string()),
                    modules: vec![],
                    quality: 0,
                }),
                Box::new(RecipeConfig {
                    recipe: "iron-plate".to_string(),
                    machine: Some("stone-furnace".to_string()),
                    modules: vec![],
                    quality: 0,
                }),
                Box::new(RecipeConfig {
                    recipe: "copper-plate".to_string(),
                    machine: Some("stone-furnace".to_string()),
                    modules: vec![],
                    quality: 0,
                }),
                Box::new(RecipeConfig {
                    recipe: "copper-cable".to_string(),
                    machine: Some("assembling-machine-2".to_string()),
                    modules: vec![],
                    quality: 0,
                }),
                Box::new(RecipeConfig {
                    recipe: "electronic-circuit".to_string(),
                    machine: Some("assembling-machine-2".to_string()),
                    modules: vec![],
                    quality: 0,
                }),
            ],
        });

        ret.factories.push(FactoryView {
            recipe_configs: vec![],
        });
        ret
    }
}

impl Default for FactorioPlanner {
    fn default() -> Self {
        Self::new(FactorioContext::load(
            &(serde_json::from_str(include_str!("../../../assets/data-raw-dump.json"))).unwrap(),
        ))
    }
}

impl Renderable for FactorioPlanner {
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
pub(crate) struct FactorioContextCreator {
    path: Option<std::path::PathBuf>,
    created_context: Option<FactorioContext>,
}

impl Renderable for FactorioContextCreator {
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
        if self.path.is_some() {
            if ui
                .button("新建上下文（阻塞！！！）")
                .on_hover_text("从所选路径加载异星工厂上下文数据")
                .clicked()
            {
                if let Some(ctx) =
                    FactorioContext::load_from_executable_path(self.path.as_ref().unwrap())
                {
                    self.created_context = Some(ctx);
                }
            }
        }
    }
}

impl GameContextCreator for FactorioContextCreator {
    fn try_create_subview(&mut self) -> Option<Box<dyn Renderable>> {
        if self.created_context.is_some() {
            return Some(Box::new(FactorioPlanner {
                item_order: None,
                ctx: self.created_context.take().unwrap(),
                factories: Vec::new(),
                selected_factory: 0,
                reverse_item_order: None,
                recipe_order: None,
                reverse_recipe_order: None,
            }));
        }
        None
    }
}
