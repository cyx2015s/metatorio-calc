use std::{collections::HashSet, sync::mpsc::*};

use crate::{
    concept::*,
    dyn_serde::*,
    factorio::{
        common::*,
        editor::{icon::*, modal::*},
        format::*,
        model::*,
        number::{CompactLabel, SignedCompactLabel},
        selector::generic_item_selector,
        style::card_frame,
    },
    solver::*,
};

use indexmap::IndexMap;

lazy_static::lazy_static! {
    static ref MECHANIC_REGISTRY: DynDeserializeRegistry<FactorioMechanic> = {
        let mut registry = DynDeserializeRegistry::default();
        RecipeMechanic::register(&mut registry);
        MiningMechanic::register(&mut registry);
        registry
    };
}

pub struct FactoryInstance {
    pub name: String,
    pub target: Vec<(GenericItem, f64)>,
    pub external: Vec<(GenericItem, f64)>,
    pub mechanics: Vec<Box<dyn Mechanic<FactorioContext, GenericItem>>>,

    pub arg_sender: Sender<SolverData<GenericItem, (usize, usize)>>,
    pub strict_source: bool,

    pub solution: (Flow<(usize, usize)>, f64),
    pub total_flow: Flow<GenericItem>,
    pub total_flow_sorted_keys: Vec<GenericItem>,
    pub solution_receiver: Receiver<SolverSolutionTuple<(usize, usize)>>,

    pub factory_sender: Option<Sender<FactoryInstance>>, // 告诉 planner view，有新工厂啦
}

impl serde::Serialize for FactoryInstance {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("FactoryInstance", 5)?;
        serde::ser::SerializeStruct::serialize_field(&mut state, "name", &self.name)?;
        serde::ser::SerializeStruct::serialize_field(&mut state, "target", &self.target)?;
        serde::ser::SerializeStruct::serialize_field(&mut state, "external", &self.external)?;
        serde::ser::SerializeStruct::serialize_field(&mut state, "mechanics", &self.mechanics)?;
        serde::ser::SerializeStruct::serialize_field(
            &mut state,
            "strict_source",
            &self.strict_source,
        )?;
        serde::ser::SerializeStruct::end(state)
    }
}

impl<'de> serde::Deserialize<'de> for FactoryInstance {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut factory_instance = FactoryInstance::default();
        let value = serde_json::Value::deserialize(deserializer)?;
        factory_instance.name =
            serde_json::from_value(value["name"].clone()).map_err(serde::de::Error::custom)?;
        factory_instance.target =
            serde_json::from_value(value["target"].clone()).map_err(serde::de::Error::custom)?;
        factory_instance.external =
            serde_json::from_value(value["external"].clone()).map_err(serde::de::Error::custom)?;
        factory_instance.strict_source = serde_json::from_value(value["strict_source"].clone())
            .map_err(serde::de::Error::custom)?;
        let mut not_deserialized_mechanics = MECHANIC_REGISTRY
            .registered_types()
            .into_iter()
            .collect::<HashSet<_>>();
        // dbg!(&not_deserialized_mechanics);
        for mechanic in value["mechanics"].as_array().unwrap_or(&vec![]) {
            if mechanic["type"]
                .as_str()
                .is_some_and(|t| not_deserialized_mechanics.contains(t))
            {
                let mech = MECHANIC_REGISTRY
                    .deserialize(mechanic.clone())
                    .map_err(|_| serde::de::Error::custom("反序列化 Mechanic 失败"))?;
                factory_instance.mechanics.push(mech);
                not_deserialized_mechanics.remove(mechanic["type"].as_str().unwrap());
                dbg!(&not_deserialized_mechanics);
            }
        }
        for not_deserialized_mechanic in not_deserialized_mechanics {
            let mech = MECHANIC_REGISTRY
                .create_default(not_deserialized_mechanic)
                .unwrap();
            factory_instance.mechanics.push(mech);
        }
        Ok(factory_instance)
    }
}

impl Clone for FactoryInstance {
    fn clone(&self) -> Self {
        FactoryInstance {
            name: self.name.clone(),
            target: self.target.clone(),
            external: self.external.clone(),
            solution: self.solution.clone(),
            total_flow: self.total_flow.clone(),
            total_flow_sorted_keys: self.total_flow_sorted_keys.clone(),
            mechanics: self.mechanics.clone(),
            factory_sender: self.factory_sender.clone(),
            ..Default::default()
        }
    }
}

impl Default for FactoryInstance {
    fn default() -> Self {
        let (arg_tx, arg_rx) = channel();
        let (solution_tx, solution_rx) = channel();
        SolverData::make_solver_thread(solution_tx, arg_rx);

        FactoryInstance {
            name: "工厂".to_string(),
            target: Vec::new(),
            external: Vec::new(),
            mechanics: Vec::new(),
            arg_sender: arg_tx,
            strict_source: false,
            solution: (IndexMap::new(), 0.0),
            total_flow: IndexMap::new(),
            total_flow_sorted_keys: Vec::new(),
            solution_receiver: solution_rx,

            factory_sender: None,
        }
    }
}

impl FactoryInstance {
    pub fn new(name: String) -> Self {
        FactoryInstance {
            name,
            ..Default::default()
        }
    }

    pub fn set_sender(mut self, sender: Sender<FactoryInstance>) -> Self {
        self.factory_sender = Some(sender);
        self
    }

    pub fn add_mechanic(mut self, mechanic: impl Mechanic<FactorioContext, GenericItem>) -> Self {
        self.mechanics.push(Box::new(mechanic));
        self
    }

    pub fn send_solve_request(&self, factorio: &FactorioContext) {
        let flows = self
            .mechanics
            .iter()
            .enumerate()
            .flat_map(move |(idx, mechanic)| {
                mechanic
                    .instances()
                    .iter()
                    .enumerate()
                    .map(move |(jdx, fe)| ((idx, jdx), (fe.as_flow(factorio), fe.cost(factorio))))
                    .collect::<Vec<_>>()
            })
            .collect();

        // .flat_map(|mechanic| mechanic.instances())
        // .map(|fe| (ref_as_ptr(fe), (fe.as_flow(factorio), fe.cost(factorio))))
        // .collect::<IndexMap<usize, (_, _)>>();
        let target = self
            .target
            .iter()
            .map(|(item, amount)| (item.clone(), *amount))
            .fold(IndexMap::new(), |mut acc, (item, amount)| {
                *acc.entry(item).or_insert(0.0) += amount;
                acc
            });
        let external = self
            .external
            .iter()
            .map(|(item, amount)| (item.clone(), *amount))
            .fold(IndexMap::new(), |mut acc, (item, amount)| {
                let v = acc.entry(item.clone()).or_default();
                if *v < amount {
                    *v = amount;
                }
                acc
            })
            .into_iter()
            .map(|(item, amount)| (item, 1024.0 / amount))
            .collect();
        let _ = self.arg_sender.send(
            SolverData::new(target, flows)
                .with_sources(external)
                .with_strict_source(self.strict_source),
        );
    }

    fn flows_panel(
        &mut self,
        ui: &mut egui::Ui,
        factorio: &FactorioContext,
        changed: &mut bool,
        need_suggestions: &mut bool,
    ) {
        ui.horizontal(|ui| {
            *changed |= ui
                .checkbox(
                    &mut self.strict_source,
                    "禁止使用定义在额外输入中以外的物品",
                )
                .changed();
            if ui.button("删除所有没用到的配方").clicked() {
                self.mechanics
                    .iter_mut()
                    .enumerate()
                    .for_each(|(idx, mechanic)| {
                        for jdx in 0..mechanic.instance_len() {
                            let solution_value =
                                self.solution.0.get(&(idx, jdx)).cloned().unwrap_or(0.0);
                            if solution_value < 1e-10 {
                                mechanic.instance_operate(jdx, &mut |_| EntryOperation::Drop);
                            }
                        }
                    });
            }
            if ui
                .button("\u{26A0}自动规划")
                .on_hover_text("\u{26A0}新工厂会出现在一个新页面中")
                .clicked()
            {
                let factorio_cloned = factorio.clone();
                let factory_cloned = self.clone();
                let factory_sender = self.factory_sender.clone();
                std::thread::spawn(move || {
                    if let Some(sender) = factory_sender {
                        let auto_planned_factory =
                            factorio_auto_planner(factory_cloned, factorio_cloned);
                        match auto_planned_factory {
                            Ok(factory) => {
                                crate::toast::info("自动规划工厂完成。");
                                let _ = sender.send(factory);
                            }
                            Err(e) => {
                                crate::toast::error(format!("自动规划工厂失败：{:?}", &e));
                                log::error!("自动规划工厂失败: {:?}", &e);
                            }
                        }
                    }
                });
            }
        });
        ui.label(format!("总代价: {:.2} | 总物料流", self.solution.1));
        ui.horizontal_wrapped(|ui| {
            card_frame(ui).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(50.0);

                for item in &self.total_flow_sorted_keys {
                    let amount = self.total_flow.get(item).cloned().unwrap_or(0.0);
                    if amount.abs() < 1e-6 {
                        continue;
                    }

                    ui.vertical(|ui| {
                        ui.add_sized([35.0, 15.0], SignedCompactLabel::new(amount));
                        ui.push_id(item, |ui| {
                            let button = ui
                                .add_sized([35.0, 35.0], GenericIcon::new(factorio, item))
                                .interact(egui::Sense::click());
                            button.context_menu(|ui| {
                                if ui.button("添加到产量目标").clicked() {
                                    self.target.push((item.clone(), 0.0));
                                    *changed = true;
                                }
                                if ui.button("添加到外部输入").clicked() {
                                    self.external.push((item.clone(), 1.0));
                                    *changed = true;
                                }
                                if ui.button("显示推荐配方").clicked() {
                                    *need_suggestions = true;
                                    self.mechanics.iter_mut().for_each(|mechanic| {
                                        mechanic.update_suggestion(factorio, item, amount)
                                    });
                                }
                            });
                            if button.clicked() {
                                *need_suggestions = true;
                                self.mechanics.iter_mut().for_each(|mechanic| {
                                    mechanic.update_suggestion(factorio, item, amount)
                                });
                            }
                        })
                    });
                    if ui.available_size_before_wrap().x < 35.0 {
                        ui.end_row();
                    }
                }
            });
        });
        ui.separator();
        self.mechanics
            .iter_mut()
            .enumerate()
            .for_each(|(idx, mechanic)| {
                for jdx in 0..mechanic.instance_len() {
                    ui.horizontal_wrapped(|ui| {
                        mechanic.instance_operate(jdx, &mut |_| {
                            let mut operation = EntryOperation::None;

                            card_frame(ui).show(ui, |ui| {
                                ui.vertical(|ui| {
                                    if ui.button("删除").clicked() {
                                        operation = EntryOperation::Drop;
                                    }
                                    if ui.button("复制").clicked() {
                                        operation = EntryOperation::Clone;
                                    }
                                    let solution_value = self.solution.0.get(&(idx, jdx)).cloned();
                                    if let Some(value) = solution_value {
                                        ui.add(CompactLabel::new(value));
                                    } else {
                                        ui.label("无解");
                                    }
                                })
                            });

                            operation
                        });

                        card_frame(ui).show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            *changed |= mechanic.instance_view(jdx, ui, factorio);
                        });
                    });
                }
            });
    }

    fn side_panel(
        &mut self,
        ui: &mut egui::Ui,
        factorio: &FactorioContext,
        changed: &mut bool,
        need_suggestions: &mut bool,
    ) {
        ui.heading("目标产量/消耗");
        self.target.retain_mut(|(item, amount)| {
            let mut deleted = false;
            ui.horizontal_top(|ui| {
                card_frame(ui).show(ui, |ui| {
                    ui.vertical(|ui| {
                        if matches!(
                            item,
                            GenericItem::Electricity
                                | GenericItem::Heat
                                | GenericItem::ItemFuel { .. }
                                | GenericItem::FluidFuel { .. }
                                | GenericItem::FluidHeat { .. }
                        ) {
                            *changed |= ui.add(drag_watt(amount).speed(10_000.0)).changed();
                        } else {
                            *changed |= ui.add(drag_value(amount).suffix("/秒")).changed();
                        }
                        if ui.button("删除").clicked() {
                            deleted = true;
                            *changed = true;
                        }
                    });
                });
                card_frame(ui).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        let mut icon = GenericIcon::new(factorio, item);
                        let solution_of_target = self.total_flow.get(item).cloned().unwrap_or(0.0);
                        let not_satisfied =
                            !float_cmp::approx_eq!(f64, solution_of_target, *amount, ulps = 6);
                        if not_satisfied {
                            icon = icon.with_stroke(egui::Stroke::new(2.0, egui::Color32::RED));
                        }

                        let mut widget = ui
                            .add_sized([35.0, 35.0], icon)
                            .interact(egui::Sense::click());
                        if not_satisfied {
                            widget = widget.on_hover_text("\u{26A0}目标已忽略");
                        }
                        if widget.clicked_by(egui::PointerButton::Secondary) {
                            *need_suggestions = true;
                            self.mechanics.iter_mut().for_each(|mechanic| {
                                mechanic.update_suggestion(
                                    factorio, item,
                                    -*amount, // 目标产量为正表示目前缺少对应数量的物品
                                )
                            });
                        }
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                *changed |= generic_item_selector(
                                    ui,
                                    factorio,
                                    item,
                                    &widget,
                                    widget.id.with("target"),
                                );
                            });
                        });
                    });
                });
            });
            !deleted
        });
        if ui.button("添加指定产物").clicked() {
            self.target
                .push((GenericItem::Item("item-unknown".into()), 1.0));
            *changed = true;
        }
        ui.separator();
        ui.heading("额外输入代价");
        ui.label("* 每区块的产量");
        self.external.retain_mut(|(item, amount)| {
            let mut deleted = false;
            ui.horizontal_top(|ui| {
                card_frame(ui).show(ui, |ui| {
                    ui.vertical(|ui| {
                        if matches!(
                            item,
                            GenericItem::Electricity | GenericItem::Heat | GenericItem::ItemFuel { .. } | GenericItem::FluidFuel { .. } | GenericItem::FluidHeat { .. }
                        ) {
                            *changed |= ui.add(drag_watt(amount).speed(10_000.0)).changed();
                        } else {
                            *changed |= ui.add(drag_value(amount).suffix("/秒")).changed();
                        }
                        if ui.button("删除").clicked() {
                            deleted = true;
                            *changed = true;
                        }
                    });
                });
                card_frame(ui).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        let mut icon = ui.add_sized([35.0, 35.0], GenericIcon::new(factorio, item)).interact(egui::Sense::click());
                        if let GenericItem::Entity(..) = item {
                            icon = icon.on_hover_text("\u{26A0}指完成机制所消耗的实体资源（主要是矿物），不包括为了完成机制所需要收集的组装机、采矿机、插件塔等。")
                        }
                        if icon.clicked_by(egui::PointerButton::Secondary) {
                            *need_suggestions = true;
                            self.mechanics.iter_mut().for_each(|mechanic| {
                                mechanic.update_suggestion(
                                    factorio, item, 1.0, // 尝试消耗更多该物品
                                )
                            });
                        }
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                *changed |= generic_item_selector(ui, factorio, item, &icon, icon.id.with("external"));
                            });
                        });
                    });
                });
            });
            !deleted
        });
        if ui.button("添加外部输入").clicked() {
            self.external
                .push((GenericItem::Item("item-unknown".into()), 1.0));
            *changed = true;
        }
        ui.menu_button("从星球自动选择", |ui| {
            for planet in factorio.planets.values() {
                if ui
                    .button(factorio.get_display_name("space-location", &planet.base.name))
                    .clicked()
                {
                    self.external.clear();
                    let available = planet.collect_autoplaced(factorio);
                    for item in &available {
                        self.external.push((
                            item.clone(),
                            match item {
                                GenericItem::Fluid { .. } => 1048576.0,
                                GenericItem::Entity(..) => 16.0,
                                GenericItem::Item(..) => 16.0,
                                _ => 1.0,
                            },
                        ));
                    }
                    self.external
                        .push((GenericItem::Electricity, 2.0_f64.powi(24)));
                    *changed = true;
                }
            }
        });
        ui.separator();
        ui.heading("游戏机制");
        for mechanic in self.mechanics.iter_mut() {
            ui.separator();
            *changed |= mechanic.editor_view(ui, factorio);
        }
    }
}

pub struct StatefulFactoryInstance {
    pub factory: FactoryInstance,
    pub saved: bool,
    pub file_path: Option<std::path::PathBuf>,
}

impl From<FactoryInstance> for StatefulFactoryInstance {
    fn from(factory: FactoryInstance) -> Self {
        Self {
            factory,
            saved: false,
            file_path: None,
        }
    }
}

pub struct PlannerView {
    /// 存储游戏逻辑数据的全部上下文
    pub factorio: FactorioContext,

    pub intercept_close: bool,

    pub factories: Vec<StatefulFactoryInstance>,

    pub selected_factory: usize,
    pub new_factory_name: String,

    pub factory_receiver: Receiver<FactoryInstance>,
    pub factory_sender: Sender<FactoryInstance>,
}

impl SolveContext for FactoryInstance {
    type Game = FactorioContext;
    type Item = GenericItem;
}

impl EditorView for FactoryInstance {
    fn editor_view(&mut self, ui: &mut egui::Ui, factorio: &FactorioContext) -> bool {
        let label = ui.add(
            egui::text_edit::TextEdit::singleline(&mut self.name).font(egui::TextStyle::Heading),
        );
        ui.separator();
        let id = label.id;
        let mut changed = false;
        let mut need_suggestions = false;
        while let Ok(result) = self.solution_receiver.try_recv() {
            match result {
                Ok(solution) => {
                    self.total_flow.clear();
                    self.solution = solution;
                    for (idx, mechanic) in self.mechanics.iter().enumerate() {
                        for (jdx, instance) in mechanic.instances().iter().enumerate() {
                            let var_value =
                                self.solution.0.get(&(idx, jdx)).cloned().unwrap_or(0.0);
                            let flow = instance.as_flow(factorio);
                            self.total_flow = flow_add(&self.total_flow, &flow, var_value);
                        }
                    }
                    // Update sorted keys cache when total_flow changes
                    self.total_flow_sorted_keys = self.total_flow.keys().cloned().collect();
                    sort_generic_items_owned(&mut self.total_flow_sorted_keys, factorio);
                    ui.memory_mut(|mem| {
                        mem.data.remove::<String>(id);
                    })
                }
                Err(err) => {
                    self.total_flow.clear();
                    self.total_flow_sorted_keys.clear();
                    self.solution.0.clear();
                    self.solution.1 = f64::NAN;
                    ui.memory_mut(|mem| {
                        mem.data.insert_temp(id, err);
                    });
                }
            }
        }

        egui::SidePanel::new(egui::containers::panel::Side::Left, egui::Id::new("target"))
            .show_separator_line(true)
            .min_width(128.0)
            .max_width(256.0)
            .frame(egui::Frame::NONE.corner_radius(8.0).inner_margin(4.0))
            .show_inside(ui, |ui: &mut egui::Ui| {
                egui::ScrollArea::vertical().id_salt(1).show(ui, |ui| {
                    self.side_panel(ui, factorio, &mut changed, &mut need_suggestions);
                });
            });

        egui::Frame::NONE
            .corner_radius(8.0)
            .outer_margin(4.0)
            .show(ui, |ui| {
                ui.heading("配方配置");
                egui::ScrollArea::vertical().id_salt(3).show(ui, |ui| {
                    ui.vertical(|ui| {
                        // Use cached sorted keys instead of sorting every frame
                        self.flows_panel(ui, factorio, &mut changed, &mut need_suggestions);
                    })
                    .response
                });
            });
        show_modal(egui::Id::new("推荐"), need_suggestions, ui, |ui| {
            ui.set_max_height(480.0);
            ui.set_min_height(480.0);
            ui.set_min_width(640.0);
            ui.set_max_width(640.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.mechanics.iter_mut().for_each(|mechanic| {
                    ui.collapsing(mechanic.name(), |ui| {
                        ui.heading(mechanic.name());
                        changed |= mechanic.suggestion_view(ui, factorio);
                        ui.separator();
                    });
                });
            });
        });
        // 无关
        if changed {
            self.send_solve_request(factorio);
        };
        changed
    }
}

impl PlannerView {
    pub fn new(factorio: FactorioContext) -> Self {
        PlannerView {
            factorio: factorio.build_order_info(),
            ..Default::default()
        }
    }
}

impl Default for PlannerView {
    fn default() -> Self {
        let (factory_tx, factory_rx) = channel();
        PlannerView {
            factorio: FactorioContext::default().build_order_info(),
            intercept_close: true,
            factories: Vec::new(),
            selected_factory: 0,
            new_factory_name: String::new(),
            factory_receiver: factory_rx,
            factory_sender: factory_tx,
        }
    }
}

impl Subview for PlannerView {
    fn view(&mut self, ui: &mut egui::Ui) {
        let mut show_close_confirm = false;
        if self.intercept_close && ui.ctx().input(|input| input.viewport().close_requested()) {
            for factory in &self.factories {
                if !factory.saved {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    show_close_confirm = true;
                    break;
                }
            }
        }
        if let Ok(factory) = self.factory_receiver.try_recv() {
            self.factories.push(StatefulFactoryInstance {
                factory,
                saved: false,
                file_path: None,
            });
        }
        show_modal(
            egui::Id::new("close-confirm"),
            show_close_confirm,
            ui,
            |ui| {
                ui.heading("有未保存的工厂，确认关闭吗？");
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        if ui.button("确认").clicked() {
                            self.intercept_close = false;
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }

                        if ui.button("取消").clicked() {
                            ui.close();
                        }
                    });
                });
            },
        );

        egui::Frame::group(ui.style())
            .corner_radius(8.0)
            .stroke(egui::Stroke::new(
                1.0,
                ui.visuals().widgets.noninteractive.fg_stroke.color,
            ))
            .show(ui, |ui| {
                egui::containers::menu::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("文件", |ui| {
                        if ui.button("新建工厂").clicked() {
                            let name = "新工厂".to_string();
                            self.factories.push(
                                FactoryInstance::new(name)
                                    .add_mechanic(RecipeMechanic::default())
                                    .add_mechanic(MiningMechanic::default())
                                    .set_sender(self.factory_sender.clone())
                                    .into(),
                            );
                        }
                        if ui.button("从文件加载工厂……").clicked()
                            && let Some(path) = rfd::FileDialog::new()
                                .add_filter("异星工厂规划配置", &["fpc", "json"])
                                .pick_file()
                        {
                            match std::fs::read_to_string(&path) {
                                Err(err) => {
                                    crate::toast::error(format!(
                                        "无法读取文件 {}: {}",
                                        path.display(),
                                        err
                                    ));
                                }
                                Ok(content) => {
                                    match serde_json::from_str::<FactoryInstance>(&content) {
                                        Err(err) => {
                                            crate::toast::error(format!(
                                                "无法解析文件 {}: {}",
                                                path.display(),
                                                err
                                            ));
                                        }
                                        Ok(factory) => {
                                            let thread_path = path.clone();
                                            std::thread::spawn(move || {
                                                std::thread::sleep(
                                                    std::time::Duration::from_millis(500),
                                                );
                                                crate::toast::success(format!(
                                                    "从 {} 加载了新工厂",
                                                    thread_path.display()
                                                ));
                                            });
                                            let factory =
                                                factory.set_sender(self.factory_sender.clone());
                                            factory.send_solve_request(&self.factorio);
                                            self.factories.push(StatefulFactoryInstance {
                                                factory,
                                                saved: true,
                                                file_path: Some(path),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    });
                });
                ui.separator();
                egui::containers::menu::MenuBar::new().ui(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mut idx = 0usize;
                        self.factories.retain_mut(|factory| {
                            let mut deleted = false;
                            let button = ui.add(
                                egui::Button::new(format!(
                                    "{}{}",
                                    factory.factory.name,
                                    if factory.saved { "" } else { " *" }
                                ))
                                .selected(self.selected_factory == idx),
                            );
                            if button.clicked() {
                                self.selected_factory = idx;
                            }
                            button.context_menu(|ui| {
                                if let Some(file_path) = factory.file_path.as_ref()
                                    && ui
                                        .add(egui::Button::new("保存").shortcut_text("Ctrl+S"))
                                        .clicked()
                                {
                                    if let Ok(()) = save_to_file(&factory.factory, file_path) {
                                        factory.saved = true;
                                        crate::toast::success(format!(
                                            "工厂已保存到 {}",
                                            file_path.display()
                                        ));
                                    }
                                    ui.close();
                                }
                                if ui
                                    .add(if factory.file_path.is_some() {
                                        egui::Button::new("另存为……")
                                    } else {
                                        egui::Button::new("保存……").shortcut_text("Ctrl+S")
                                    })
                                    .clicked()
                                {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("异星工厂规划配置", &["fpc", "json"])
                                        .set_file_name(
                                            format!("{}.fpc", &factory.factory.name).as_str(),
                                        )
                                        .save_file()
                                        && let Ok(()) = save_to_file(&factory.factory, &path)
                                    {
                                        factory.saved = true;
                                        factory.file_path = Some(path.clone());
                                        crate::toast::success(format!(
                                            "工厂已保存到 {}",
                                            path.display()
                                        ));
                                    }
                                    ui.close();
                                }

                                if ui.button("关闭").clicked() {
                                    deleted = true;
                                    if self.selected_factory >= idx && self.selected_factory > 0 {
                                        self.selected_factory -= 1;
                                    }
                                    idx -= 1;
                                }
                            });
                            idx += 1;
                            !deleted
                        })
                    });
                });
                ui.separator();
                if self.factories.is_empty() {
                    let mut layout_job = egui::text::LayoutJob::default();
                    egui::RichText::new("没有工厂\n").size(32.0).append_to(
                        &mut layout_job,
                        ui.style(),
                        egui::FontSelection::Default,
                        egui::Align::Center,
                    );
                    egui::RichText::new("点击上方的文件菜单新建工厂或加载一个工厂存档。")
                        .append_to(
                            &mut layout_job,
                            ui.style(),
                            egui::FontSelection::Default,
                            egui::Align::Center,
                        );
                    ui.add_sized(ui.available_size(), egui::Label::new(layout_job));
                } else {
                    let factory = &mut self.factories[self.selected_factory];
                    factory.saved &= !factory.factory.editor_view(ui, &self.factorio);
                    if ui
                        .ctx()
                        .input(|input| input.modifiers.command && input.key_pressed(egui::Key::S))
                        && !factory.saved
                    {
                        if factory.file_path.is_none() {
                            let file_path = rfd::FileDialog::new()
                                .add_filter("异星工厂规划配置", &["fpc", "json"])
                                .set_file_name(format!("{}.fpc", &factory.factory.name).as_str())
                                .save_file();
                            factory.file_path = file_path;
                        }
                        if let Some(path) = factory.file_path.as_ref()
                            && let Ok(()) = save_to_file(&factory.factory, path)
                        {
                            crate::toast::success(format!("工厂已保存到 {}", path.display()));
                            factory.saved = true;
                        }
                    }
                }
            });
    }

    fn name(&self) -> String {
        "异星工厂 - 工厂规划器".to_string()
    }

    fn description(&self) -> String {
        self.factorio.mods.iter().fold(
            "使用以下模组: ".to_string(),
            |mut acc, (mod_name, mod_version)| {
                acc.push_str(&format!("\n{} ({}), ", mod_name, mod_version));
                acc
            },
        )
    }
}

#[derive(Default, Debug)]
pub struct FactorioContextCreatorView {
    path: Option<std::path::PathBuf>,
    mod_path: Option<std::path::PathBuf>,
    subview_sender: Option<Sender<Box<dyn Subview>>>,
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
                    ui.label("若为 Steam 版本的游戏，请关闭正在运行中的异星工厂并且启动 Steam 再执行加载游戏上下文");
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

            let mut can_load_context = true;
            if self.path.is_none() {
                ui.label("请选择游戏可执行文件以继续。");
                can_load_context = false;
            }
            if let Some(mod_path) = self.mod_path.as_ref()
                && !mod_path.join("mod-list.json").exists()
            {
                ui.label("模组文件夹下未找到 mod-list.json。");
                can_load_context = false;
            }

            if self.thread.is_some() {
                ui.label("正在加载游戏上下文，请稍候...");
                can_load_context = false;
            }

            ui.separator();

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
                self.thread =
                    Some(std::thread::spawn(
                        move || match FactorioContext::load_from_executable_path(
                            &exe_path,
                            mod_path.as_deref(),
                            None,
                        ) {
                            Ok(factorio) => {
                                sender
                                    .send(Box::new(PlannerView::new(factorio)))
                                    .expect("Failed to send subview");
                            }
                            Err(e) => {
                                crate::toast::error(format!("加载游戏上下文失败: {:?}", e));
                            }
                        },
                    ));
            }

            ui.separator();

            if ui
                .add_enabled(self.thread.is_none(), egui::Button::new("加载缓存上下文"))
                .clicked()
                && let Some(sender) = &self.subview_sender
                && let None = self.thread
            {
                let sender = sender.clone();
                self.thread =
                    Some(std::thread::spawn(
                        move || match FactorioContext::load_from_tmp_no_dump() {
                            Ok(factorio) => {
                                sender.send(Box::new(PlannerView::new(factorio))).unwrap();
                            }
                            Err(e) => {
                                crate::toast::error(format!("加载缓存上下文失败: {:?}", e));
                            }
                        },
                    ));
            }
            if let Some(ref thread) = self.thread
                && thread.is_finished()
            {
                let thread = self.thread.take().unwrap();
                thread.join().unwrap();
            }
        });
    }
}

impl GameContextCreatorView for FactorioContextCreatorView {
    fn set_subview_sender(&mut self, sender: Sender<Box<dyn Subview>>) {
        self.subview_sender = Some(sender);
    }
}
