use crate::{
    context::RecipeLike,
    ctx::factorio::{context::FactorioContext, recipe::RecipeConfig},
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

pub(crate) struct FactorioPlannerView {
    /// 存储游戏逻辑数据的全部上下文
    pub(crate) ctx: FactorioContext,
    pub(crate) factories: Vec<FactoryView>,
    pub(crate) selected_factory: usize,
}

impl FactorioPlannerView {
    pub(crate) fn new(ctx: FactorioContext) -> Self {
        let mut ret = FactorioPlannerView {
            ctx,
            factories: Vec::new(),
            selected_factory: 0,
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

impl Default for FactorioPlannerView {
    fn default() -> Self {
        Self::new(FactorioContext::load(
            &(serde_json::from_str(include_str!("../../../assets/data-raw-dump.json"))).unwrap(),
        ))
    }
}

impl eframe::App for FactorioPlannerView {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
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
            for config in self.factories[self.selected_factory].recipe_configs.iter() {
                config.ui(ui, &self.ctx);
            }
        });
    }
}
