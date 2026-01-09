use egui::ScrollArea;

use crate::{
    concept::{AsFlowEditor, AsFlowSource, ContextBound, EditorView},
    factorio::model::{
        context::{FactorioContext, GenericItem},
        recipe::RecipeConfigSource,
    },
};

pub struct FactoryInstance {
    pub name: String,
    pub flow_sources:
        Vec<Box<dyn AsFlowSource<ContextType = FactorioContext, ItemIdentType = GenericItem>>>,
    pub flow_configs:
        Vec<Box<dyn AsFlowEditor<ItemIdentType = GenericItem, ContextType = FactorioContext>>>,
    pub flow_receiver: std::sync::mpsc::Receiver<
        Box<dyn AsFlowEditor<ItemIdentType = GenericItem, ContextType = FactorioContext>>,
    >,
    pub flow_sender: std::sync::mpsc::Sender<
        Box<dyn AsFlowEditor<ItemIdentType = GenericItem, ContextType = FactorioContext>>,
    >,
}

pub struct PlannerView {
    /// 存储游戏逻辑数据的全部上下文
    pub ctx: FactorioContext,

    pub factories: Vec<FactoryInstance>,
    pub selected_factory: usize,
}

impl ContextBound for FactoryInstance {
    type ContextType = FactorioContext;
    type ItemIdentType = GenericItem;
}

impl EditorView for FactoryInstance {
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &FactorioContext) {
        ui.heading(&self.name);
        for recipe_source in &mut self.flow_sources {
            recipe_source.editor_view(ui, ctx);
            ui.separator();
        }
        while let Ok(flow_source) = self.flow_receiver.try_recv() {
            self.flow_configs.push(flow_source);
        }

        ScrollArea::vertical().show(ui, |ui| {
            for recipe_config in &mut self.flow_configs {
                recipe_config.editor_view(ui, ctx);
                ui.separator();
            }
        });
    }
}

use crate::{
    concept::GameContextCreatorView,
    concept::Subview,
    factorio::{
        common::Effect,
        model::{module::ModuleConfig, recipe::RecipeConfig},
    },
};

impl PlannerView {
    pub fn new(ctx: FactorioContext) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();

        let mut ret = PlannerView {
            ctx: ctx.build_order_info(),
            factories: Vec::new(),
            selected_factory: 0,
        };
        ret.factories.push(FactoryInstance {
            name: "工厂".to_string(),
            flow_sources: vec![Box::new(RecipeConfigSource {
                editing: RecipeConfig::default(),
                sender: tx.clone(),
            })],
            flow_sender: tx,
            flow_receiver: rx,
            flow_configs: vec![
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
        Self::new(FactorioContext::load(
            &(serde_json::from_str(include_str!("../../../assets/data-raw-dump.json"))).unwrap(),
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
            self.factories[self.selected_factory].editor_view(ui, &self.ctx);
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
                    if let Some(ctx) = FactorioContext::load_from_executable_path(
                        &exe_path,
                        mod_path.as_deref(),
                        None,
                    ) {
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
                    if let Some(ctx) = FactorioContext::load_from_tmp_no_dump() {
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
