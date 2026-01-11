use std::collections::HashMap;

use crate::{
    concept::{AsFlowEditor, AsFlowEditorSource, AsFlowSender, ContextBound, EditorView},
    factorio::{
        common::sort_generic_items,
        editor::{icon::GenericIcon, selector::selector_menu_with_filter},
        format::{CompactLabel, SignedCompactLabel},
        model::{
            context::{FactorioContext, GenericItem},
            recipe::RecipeConfigSource,
            source::SourceConfigSource,
        },
    },
    solver::{basic_solver, hash_map_add},
};

pub struct FactoryInstance {
    pub name: String,
    pub target: Vec<(GenericItem, f64)>,
    pub total_flow: HashMap<GenericItem, f64>,
    pub flow_editor_sources: Vec<
        Box<dyn AsFlowEditorSource<ContextType = FactorioContext, ItemIdentType = GenericItem>>,
    >,
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
            target: Vec::new(),
            total_flow: HashMap::new(),
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
            target: Vec::new(),
            total_flow: HashMap::new(),
            flow_editor_sources: Vec::new(),
            flow_editors: Vec::new(),
            flow_receiver: rx,
            flow_sender: tx,
        }
    }
    pub fn add_flow_source<
        F: Fn(
            AsFlowSender<GenericItem, FactorioContext>,
        ) -> Box<
            dyn AsFlowEditorSource<ContextType = FactorioContext, ItemIdentType = GenericItem>,
        >,
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
        let id = ui.id();
        // FIXME
        // 主线程算东西之后会卡死的，现在先这样
        if ui.ctx().cumulative_frame_nr().is_multiple_of(10) {
            let flows = self
                .flow_editors
                .iter()
                .map(|fe| (fe.as_flow(ctx), fe.cost(ctx)))
                .enumerate()
                .collect::<HashMap<usize, (_, _)>>();
            let target = self
                .target
                .iter()
                .map(|(item, amount)| (item.clone(), *amount))
                .fold(HashMap::new(), |mut acc, (item, amount)| {
                    *acc.entry(item).or_insert(0.0) += amount;
                    acc
                });
            let result = basic_solver(target, flows);
            match result {
                Ok(solution) => {
                    self.total_flow = HashMap::new();
                    for (idx, value) in solution.iter() {
                        self.flow_editors[*idx].notify_solution(*value);
                        self.total_flow = hash_map_add(
                            &self.total_flow,
                            &self.flow_editors[*idx].as_flow(ctx),
                            *value,
                        );
                    }
                    ui.memory_mut(|mem| {
                        mem.data.remove::<String>(id);
                    })
                }
                Err(err) => {
                    self.total_flow = HashMap::new();
                    ui.memory_mut(|mem| {
                        mem.data.insert_temp(id, err);
                    });
                }
            }
        }
        let err_info = ui.memory(|mem| mem.data.get_temp::<String>(id)).clone();
        if let Some(err_info) = &err_info {
            ui.label(format!("求解错误: {}", err_info));
        }

        let split_ratio = ui.memory(|mem| {
            mem.data
                .get_temp(ui.id())
                .unwrap_or(FactoryInstancePanelSplitInfo { h: 0.4, v: 0.4 })
        });
        let max_rect = ui.available_rect_before_wrap();
        let (left_panel, flows_panel) = max_rect.split_left_right_at_fraction(split_ratio.h);
        let (target_panel, source_panel) = left_panel.split_top_bottom_at_fraction(split_ratio.v);

        ui.put(target_panel.shrink(4.0), |ui: &mut egui::Ui| {
            egui::ScrollArea::vertical()
                .id_salt(1)
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.heading("优化目标");
                            let mut delete_target: Option<usize> = None;
                            for (idx, (item, amount)) in self.target.iter_mut().enumerate() {
                                ui.horizontal_top(|ui| {
                                    let icon = ui
                                        .add_sized(
                                            [35.0, 35.0],
                                            GenericIcon {
                                                ctx,
                                                item,
                                                size: 32.0,
                                            },
                                        )
                                        .interact(egui::Sense::click());
                                    ui.vertical(|ui| {
                                        let label = ui.label("选择目标产物类型");
                                        egui::ComboBox::from_id_salt(label.id)
                                            .selected_text(match item {
                                                GenericItem::Item { .. } => "物体",
                                                GenericItem::Fluid { .. } => "流体",
                                                GenericItem::Entity { .. } => "实体",
                                                GenericItem::Heat => "热量",
                                                GenericItem::Electricity => "电力",
                                                GenericItem::FluidHeat { .. } => "流体热量",
                                                GenericItem::FluidFuel { .. } => "流体燃料",
                                                GenericItem::ItemFuel { .. } => "物体燃料",
                                                GenericItem::RocketPayloadWeight => "重量载荷",
                                                GenericItem::RocketPayloadStack => "堆叠载荷",
                                                GenericItem::Pollution { .. } => "污染",
                                                _ => "特殊",
                                            })
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(
                                                    item,
                                                    GenericItem::Item {
                                                        name: "item-unknown".to_string(),
                                                        quality: 0,
                                                    },
                                                    "物体",
                                                );
                                                ui.selectable_value(
                                                    item,
                                                    GenericItem::Fluid {
                                                        name: "fluid-unknown".to_string(),
                                                        temperature: None,
                                                    },
                                                    "流体",
                                                );
                                            });
                                    });
                                    match item {
                                        GenericItem::Item { name: _, quality } => {
                                            if let Some(selected) = selector_menu_with_filter(
                                                ui,
                                                ctx,
                                                "选择物品",
                                                "item",
                                                icon,
                                            ) {
                                                *item = GenericItem::Item {
                                                    name: selected,
                                                    quality: *quality,
                                                };
                                            }
                                        }
                                        GenericItem::Fluid {
                                            name: _,
                                            temperature,
                                        } => {
                                            if let Some(selected) = selector_menu_with_filter(
                                                ui,
                                                ctx,
                                                "选择流体",
                                                "fluid",
                                                icon,
                                            ) {
                                                *item = GenericItem::Fluid {
                                                    name: selected,
                                                    temperature: *temperature,
                                                };
                                            }
                                        }
                                        _ => {}
                                    }
                                    ui.vertical(|ui| {
                                        ui.label("目标产量");
                                        ui.add(egui::DragValue::new(amount).suffix("/s"));
                                    });
                                    if ui.button("删除").clicked() {
                                        delete_target = Some(idx);
                                    }
                                });
                            }
                            if let Some(idx) = delete_target {
                                self.target.remove(idx);
                            }
                            if ui.button("添加目标产物").clicked() {
                                self.target.push((
                                    GenericItem::Item {
                                        name: "item-unknown".to_string(),
                                        quality: 0,
                                    },
                                    1.0,
                                ));
                            }
                        })
                        .response
                    })
                    .inner
                })
                .inner
        });

        ui.put(source_panel.shrink(4.0), |ui: &mut egui::Ui| {
            egui::ScrollArea::vertical()
                .id_salt(2)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.heading("游戏机制");
                        for flow_source in &mut self.flow_editor_sources {
                            flow_source.editor_view(ui, ctx);
                            ui.separator();
                        }
                    })
                    .response
                })
                .inner
        });
        let mut delete_flow: Option<usize> = None;
        ui.put(flows_panel.shrink(4.0), |ui: &mut egui::Ui| {
            egui::ScrollArea::vertical()
                .id_salt(3)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        let mut keys = self.total_flow.keys().collect::<Vec<_>>();
                        sort_generic_items(&mut keys, ctx);
                        ui.horizontal_wrapped(|ui| {
                            for item in keys {
                                let mut amount = self.total_flow.get(item).cloned().unwrap_or(0.0);
                                if amount.abs() < 1e-9 {
                                    amount = amount.abs();
                                }
                                ui.vertical(|ui| {
                                    ui.add_sized([35.0, 15.0], SignedCompactLabel::new(amount));
                                    ui.add_sized(
                                        [35.0, 35.0],
                                        GenericIcon {
                                            ctx,
                                            item,
                                            size: 32.0,
                                        },
                                    );
                                    if ui.available_size_before_wrap().x < 35.0 {
                                        ui.end_row();
                                    }
                                });
                            }
                        });
                        
                        ui.heading("配方配置");
                        for (i, flow_config) in self.flow_editors.iter_mut().enumerate() {
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    if ui.button("删除").clicked() {
                                        delete_flow = Some(i);
                                    }
                                    if err_info.is_none() && let Some(solution) = flow_config.get_solution() {
                                        ui.add(CompactLabel::new(solution));
                                    } else {
                                        ui.label("待解");
                                    }
                                });

                                ui.separator();
                                ui.vertical(|ui| flow_config.editor_view(ui, ctx));
                            });
                        }
                    })
                    .response
                })
                .inner
        });
        if let Some(idx) = delete_flow {
            self.flow_editors.remove(idx);
        }

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
        ui.button("模组版本信息").on_hover_ui(|ui| {
            for (mod_name, mod_version) in &self.ctx.mods {
                ui.label(format!("模组 {} 版本 {}", mod_name, mod_version));
            }
        });
        ui.separator();
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
                self.factories.push(
                    FactoryInstance::new(name)
                        .add_flow_source(|s| {
                            Box::new(RecipeConfigSource {
                                editing: RecipeConfig::default(),
                                sender: s,
                            })
                        })
                        .add_flow_source(|s| Box::new(SourceConfigSource { sender: s })),
                );
                self.new_factory_name.clear();
            }
        });
        ui.separator();
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
            ui.heading("创建游戏上下文");
            ui.separator();

            ui.label("选择游戏路径:");
            if ui.button("浏览...").clicked()
                && let Some(path) = rfd::FileDialog::new().pick_file()
            {
                self.path = Some(path);
            }
            if let Some(path) = &self.path {
                ui.label(format!("已选择路径: {}", path.display()));
                if path.to_string_lossy().contains("steam") {
                    ui.label("若为 Steam 版本的游戏，请启动 Steam 再执行加载游戏上下文");
                }
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
            let mut can_load_context = true;
            if self.path.is_none() {
                ui.label("请选择游戏可执行文件以继续。");
                can_load_context = false;
            }
            if self.thread.is_some() {
                ui.label("有一个正在加载的游戏上下文了。");
                can_load_context = false;
            }
            if let Some(mod_path) = self.mod_path.as_ref()
                && !mod_path.join("mod-list.json").exists()
            {
                ui.label("模组文件夹下未找到 mod-list.json。");
                can_load_context = false;
            }
            if ui
                .add_enabled(can_load_context, egui::Button::new("加载游戏上下文"))
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
