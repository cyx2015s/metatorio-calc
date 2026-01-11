use crate::{
    concept::{AsFlowSender, AsFlowEditor, AsFlowEditorSource, ContextBound, EditorView},
    factorio::model::{
        context::{FactorioContext, GenericItem},
        recipe::RecipeConfigSource, source::SourceConfigSource,
    },
};

pub struct FactoryInstance {
    pub name: String,
    pub flow_editor_sources:
        Vec<Box<dyn AsFlowEditorSource<ContextType = FactorioContext, ItemIdentType = GenericItem>>>,
    pub flow_editors:
        Vec<Box<dyn AsFlowEditor<ItemIdentType = GenericItem, ContextType = FactorioContext>>>,
    pub flow_receiver: std::sync::mpsc::Receiver<
        Box<dyn AsFlowEditor<ItemIdentType = GenericItem, ContextType = FactorioContext>>,
    >,
    pub flow_sender: std::sync::mpsc::Sender<
        Box<dyn AsFlowEditor<ItemIdentType = GenericItem, ContextType = FactorioContext>>,
    >,
}
impl Default for FactoryInstance {
    fn default() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        FactoryInstance {
            name: "工厂".to_string(),
            flow_editor_sources: Vec::new(),
            flow_editors: Vec::new(),
            flow_receiver: rx,
            flow_sender: tx,
        }
    }
}

impl FactoryInstance {
    pub fn new(name: String) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        FactoryInstance {
            name,
            flow_editor_sources: Vec::new(),
            flow_editors: Vec::new(),
            flow_receiver: rx,
            flow_sender: tx,
        }
    }
    pub fn add_flow_source<
        F: Fn(
            AsFlowSender<GenericItem, FactorioContext>,
        )
            -> Box<dyn AsFlowEditorSource<ContextType = FactorioContext, ItemIdentType = GenericItem>>,
    >(
        mut self,
        f: F,
    ) -> Self {
        self.flow_editor_sources.push(f(self.flow_sender.clone()));
        self
    }
}

pub struct PlannerView {
    /// 存储游戏逻辑数据的全部上下文
    pub ctx: FactorioContext,

    pub factories: Vec<FactoryInstance>,
    pub selected_factory: usize,
    pub new_factory_name: String,
}

impl ContextBound for FactoryInstance {
    type ContextType = FactorioContext;
    type ItemIdentType = GenericItem;
}

/// 信息用于保存工厂面板的拆分状态，不用 pub
#[derive(Debug, Clone, Copy)]
struct FactoryInstancePanelSplitInfo {
    pub h: f32,
    pub v: f32,
}

impl EditorView for FactoryInstance {
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &FactorioContext) {
        ui.heading(&self.name);
        let split_ratio = ui.memory(|mem| {
            mem.data
                .get_temp(ui.id())
                .unwrap_or(FactoryInstancePanelSplitInfo { h: 0.4, v: 0.4 })
        });
        let max_rect = ui.available_rect_before_wrap();
        let (left_panel, flows_panel) = max_rect.split_left_right_at_fraction(split_ratio.h);
        let (target_panel, source_panel) = left_panel.split_top_bottom_at_fraction(split_ratio.v);

        ui.put(target_panel, |ui: &mut egui::Ui| {
            egui::ScrollArea::vertical().show(ui, |ui|{
                ui.vertical(|ui| {
                    ui.heading("优化目标");
                }).response
            }).inner
        });

        ui.put(source_panel, |ui: &mut egui::Ui| {
            egui::ScrollArea::vertical().show(ui, |ui|{
                ui.vertical(|ui| {
                    ui.heading("游戏机制");
                    for flow_source in &mut self.flow_editor_sources {
                        flow_source.editor_view(ui, ctx);
                        ui.separator();
                    }
                    
                }).response
            }).inner
        });

        ui.put(flows_panel, |ui: &mut egui::Ui| {
            egui::ScrollArea::vertical().show(ui, |ui|{
                ui.vertical(|ui| {
                    ui.heading("配方配置");
                    for flow_config in &mut self.flow_editors {
                        flow_config.editor_view(ui, ctx);
                        ui.separator();
                    }
                }).response
            }).inner
        });

        while let Ok(flow_source) = self.flow_receiver.try_recv() {
            self.flow_editors.push(flow_source);
        }
    }
}

use crate::{
    concept::GameContextCreatorView, concept::Subview, factorio::model::recipe::RecipeConfig,
};

impl PlannerView {
    pub fn new(ctx: FactorioContext) -> Self {
        PlannerView {
            ctx: ctx.build_order_info(),
            factories: Vec::new(),
            selected_factory: 0,
            new_factory_name: String::new(),
        }
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
            ui.text_edit_singleline(&mut self.new_factory_name);
            if ui.button("添加工厂").clicked() {
                let name = if self.new_factory_name.is_empty() {
                    format!("工厂 {}", self.factories.len() + 1)
                } else {
                    self.new_factory_name.clone()
                };
                self.factories
                    .push(FactoryInstance::new(name).add_flow_source(|s| {
                        Box::new(RecipeConfigSource {
                            editing: RecipeConfig::default(),
                            sender: s,
                        })
                    }).add_flow_source(|s| {
                        Box::new(SourceConfigSource {
                            sender: s
                        })
                    }));
                self.new_factory_name.clear();
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
