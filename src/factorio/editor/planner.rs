use std::{
    collections::HashSet,
    io::BufReader,
    path::{Path, PathBuf},
    sync::{Arc, mpsc::*},
};

use crate::{
    concept::*,
    dyn_serde::*,
    factorio::{
        UserContext,
        common::*,
        editor::{icon::*, modal::*},
        format::*,
        model::*,
        number::{CompactLabel, SignedCompactLabel},
        selector::generic_item_selector,
        setting::UserContextEditor,
        style::card_frame,
    },
    math::IndexedVec,
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

#[derive(Debug)]
pub struct FactoryInstance {
    pub name: String,
    pub target: IndexedVec<(GenericItem, f64)>,
    pub external: IndexedVec<(GenericItem, f64)>,
    pub mechanics: Vec<Box<dyn Mechanic<FactorioContext, GenericItem>>>,
    pub instances: Vec<(usize, usize)>,

    pub arg_sender: Sender<SolverData<GenericItem, (usize, usize)>>,
    pub strict_source: bool,

    pub solution: (Flow<(usize, usize)>, f64),
    pub total_flow: Flow<GenericItem>,
    pub total_flow_sorted_keys: Vec<GenericItem>,

    pub solution_receiver: Receiver<SolverSolutionTuple<(usize, usize)>>,

    pub factory_sender: Option<Sender<FactoryInstance>>, // 往外通知，有新工厂啦
}

impl serde::Serialize for FactoryInstance {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("FactoryInstance", 6)?;
        serde::ser::SerializeStruct::serialize_field(&mut state, "name", &self.name)?;
        serde::ser::SerializeStruct::serialize_field(&mut state, "target", &self.target)?;
        serde::ser::SerializeStruct::serialize_field(&mut state, "external", &self.external)?;
        serde::ser::SerializeStruct::serialize_field(&mut state, "mechanics", &self.mechanics)?;
        serde::ser::SerializeStruct::serialize_field(&mut state, "instances", &self.instances)?;
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
        factory_instance.name = serde_json::from_value(value["name"].clone()).unwrap_or_default();
        factory_instance.target =
            serde_json::from_value(value["target"].clone()).unwrap_or_default();
        factory_instance.external =
            serde_json::from_value(value["external"].clone()).unwrap_or_default();
        factory_instance.strict_source =
            serde_json::from_value(value["strict_source"].clone()).unwrap_or_default();
        factory_instance.instances =
            serde_json::from_value(value["instances"].clone()).unwrap_or_default();
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
                let mech = MECHANIC_REGISTRY.deserialize(mechanic.clone()).unwrap();
                factory_instance.mechanics.push(mech);
                not_deserialized_mechanics.remove(mechanic["type"].as_str().unwrap());
                // dbg!(&not_deserialized_mechanics);
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
            instances: self.instances.clone(),
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
            target: IndexedVec::new(),
            external: IndexedVec::new(),
            mechanics: Vec::new(),
            instances: Vec::new(),
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

    pub fn set_sender(&mut self, sender: Sender<FactoryInstance>) {
        self.factory_sender = Some(sender);
    }

    pub fn with_sender(mut self, sender: Sender<FactoryInstance>) -> Self {
        self.factory_sender = Some(sender);
        self
    }

    pub fn with_mechanic(mut self, mechanic: impl Mechanic<FactorioContext, GenericItem>) -> Self {
        self.mechanics.push(Box::new(mechanic));
        self
    }

    pub fn reset_instances(&mut self) {
        self.instances.retain(|(idx, jdx)| {
            if *idx >= self.mechanics.len() {
                return false;
            }
            if *jdx >= self.mechanics[*idx].instance_len() {
                return false;
            }
            true
        });
        for (idx, mechanic) in self.mechanics.iter().enumerate() {
            for jdx in 0..mechanic.instance_len() {
                if !self.instances.contains(&(idx, jdx)) {
                    self.instances.push((idx, jdx));
                }
            }
        }
    }

    pub fn send_solve_request(&mut self, factorio: &FactorioContext) {
        if self
            .mechanics
            .iter()
            .map(|m| m.instance_len())
            .sum::<usize>()
            != self.instances.len()
        {
            self.reset_instances();
        }
        let flows = self
            .instances
            .iter()
            .map(|(idx, jdx)| {
                let fe = &self.mechanics[*idx].instances()[*jdx];
                ((*idx, *jdx), (fe.as_flow(factorio), fe.cost(factorio)))
            })
            .collect();

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
        egui_dnd::dnd(ui, "instances").show_vec(
            &mut self.instances,
            |ui, &mut (idx, jdx), handle, _| {
                let solution_value = self.solution.0.get(&(idx, jdx)).cloned();
                ui.horizontal_wrapped(|ui| {
                    card_frame(ui).show(ui, |ui| {
                        handle.ui(ui, |ui| {
                            ui.heading("≡");
                        });
                        ui.vertical(|ui| {
                            let button = ui.add_sized([28.0, 14.0], egui::Button::new("⧉"));
                            if button.clicked() {
                                self.mechanics[idx]
                                    .instance_operate(jdx, &mut |_| EntryOpRequest::Clone);
                            }
                            let button = ui.add_sized([28.0, 14.0], egui::Button::new("🗑"));
                            if button.clicked() {
                                self.mechanics[idx]
                                    .instance_operate(jdx, &mut |_| EntryOpRequest::Drop);
                            }
                            if let Some(value) = solution_value {
                                ui.add(CompactLabel::new(value));
                            } else {
                                ui.label("无解");
                            }
                        });
                    });
                    card_frame(ui).show(ui, |ui| {
                        let target_width = ui.available_width() * 0.3;
                        ui.set_min_width(target_width);
                        ui.set_max_width(target_width);
                        *changed |= self.mechanics[idx].instance_view(jdx, ui, factorio);
                    });
                    card_frame(ui).show(ui, |ui| {
                        let target_width = ui.available_width();
                        ui.set_min_width(target_width);
                        ui.set_max_width(target_width);
                        let flow = self.mechanics[idx].instances()[jdx].as_flow(factorio);
                        let mut flow_keys = flow.keys().cloned().collect::<Vec<_>>();
                        sort_generic_items_owned(&mut flow_keys, factorio);
                        // 先展示输入，再展示输出
                        for item in &flow_keys {
                            let amount = flow.get(item).cloned().unwrap_or(0.0);
                            if amount.abs() < 1e-8 {
                                continue;
                            }
                            ui.vertical(|ui| {
                                ui.set_min_width(35.0);
                                ui.set_max_width(35.0);
                                let icon = ui
                                    .add_sized([25.0, 25.0], GenericIcon::new(factorio, item))
                                    .interact(egui::Sense::click());
                                if icon.clicked() || icon.secondary_clicked() {
                                    *need_suggestions = true;
                                    self.mechanics.iter_mut().for_each(|mechanic| {
                                        mechanic.update_suggestion(
                                            factorio, item,
                                            -amount, // 流出表示目前缺少对应数量的物品
                                        )
                                    });
                                }

                                ui.add(SignedCompactLabel::new(
                                    amount * solution_value.unwrap_or(1.0),
                                ));
                            });
                            if ui.available_size_before_wrap().x < 35.0 {
                                ui.end_row();
                                ui.add_space(4.0);
                            }
                        }
                        // });
                    })
                });
            },
        );
    }

    fn summary_panel(
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
                                mechanic.instance_operate(jdx, &mut |_| EntryOpRequest::Drop);
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
                                crate::toast::error(format!("自动规划工厂失败：{:?}\n", &e));
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
                    if amount.abs() < 1e-8 {
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
    }

    fn side_panel(
        &mut self,
        ui: &mut egui::Ui,
        factorio: &FactorioContext,
        changed: &mut bool,
        need_suggestions: &mut bool,
    ) {
        ui.scope(|ui| {
            self.target_editor(ui, factorio, changed, need_suggestions);
        });

        ui.separator();
        ui.scope(|ui| {
            self.external_editor(ui, factorio, changed, need_suggestions);
        });
        ui.separator();
        ui.heading("游戏机制");
        for mechanic in self.mechanics.iter_mut() {
            ui.separator();
            *changed |= mechanic.editor_view(ui, factorio);
        }
        let mut results = Vec::new();
        for mechanic in self.mechanics.iter_mut() {
            results.push(mechanic.submit_operations());
        }
        for (idx, results) in results.into_iter().enumerate() {
            for result in results {
                match result {
                    EntryOpResult::Drop {
                        removed,
                        replaced_by,
                    } => {
                        *changed = true;

                        self.instances.retain_mut(|(m_idx, jdx)| {
                            let mut keep = true;
                            if *m_idx == idx {
                                if *jdx == removed {
                                    keep = false;
                                }
                                if Some(*jdx) == replaced_by {
                                    *jdx = removed;
                                }
                            }
                            keep
                        });
                    }
                    EntryOpResult::Clone { original, new } => {
                        *changed = true;
                        for i in (0..self.instances.len()).rev() {
                            if self.instances[i] == (idx, original) {
                                self.instances.insert(i + 1, (idx, new));
                                break;
                            }
                        }
                    }
                }
            }
        }
        if self
            .mechanics
            .iter()
            .map(|m| m.instance_len())
            .sum::<usize>()
            != self.instances.len()
        {
            self.reset_instances();
        }
    }

    fn external_editor(
        &mut self,
        ui: &mut egui::Ui,
        factorio: &FactorioContext,
        changed: &mut bool,
        need_suggestions: &mut bool,
    ) {
        let data = &factorio.data;
        ui.heading("额外输入代价");
        self.external
            .dnd(ui, "external", |ui, _, (item, amount), handle, _, op| {
                card_frame(ui).show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.set_min_width(ui.available_width());

                        handle.ui(ui, |ui| {
                            ui.heading("≡");
                        });
                        if item.is_energy() {
                            *changed |= ui.add(drag_watt(amount).speed(10_000.0)).changed();
                        } else {
                            *changed |= ui.add(drag_value(amount).suffix("/秒")).changed();
                        }

                        if ui.button("×").clicked() {
                            *op = EntryOpRequest::Drop;
                            *changed = true;
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        let icon = ui
                            .add_sized([35.0, 35.0], GenericIcon::new(factorio, item))
                            .interact(egui::Sense::click());
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
                                *changed |= generic_item_selector(
                                    ui,
                                    factorio,
                                    item,
                                    &icon,
                                    icon.id.with("external"),
                                );
                            });
                        });
                    });
                });
            });
        if ui.button("添加外部输入").clicked() {
            self.external
                .push((GenericItem::Item("item-unknown".into()), 1.0));
            *changed = true;
        }
        ui.menu_button("从星球自动选择", |ui| {
            for planet in factorio.data.planets.values() {
                if ui
                    .button(data.get_display_name("space-location", &planet.base.name))
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
    }

    fn target_editor(
        &mut self,
        ui: &mut egui::Ui,
        factorio: &FactorioContext,
        changed: &mut bool,
        need_suggestions: &mut bool,
    ) {
        ui.heading("目标产量/消耗");

        self.target
            .dnd(ui, "target", |ui, _, (item, amount), handle, _, op| {
                card_frame(ui).show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.set_min_width(ui.available_width());
                        handle.ui(ui, |ui| {
                            ui.heading("≡");
                        });
                        if item.is_energy() {
                            *changed |= ui.add(drag_watt(amount).speed(10_000.0)).changed();
                        } else {
                            *changed |= ui.add(drag_value(amount).suffix("/秒")).changed();
                        }
                        if ui.button("×").clicked() {
                            *op = EntryOpRequest::Drop;
                            *changed = true;
                        }
                    });
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
        if ui.button("添加指定产物").clicked() {
            self.target
                .push((GenericItem::Item("item-unknown".into()), 1.0));
            *changed = true;
        }
    }
}

impl SolveContext for FactoryInstance {
    type Game = FactorioContext;
    type Item = GenericItem;
}

impl EditorView for FactoryInstance {
    fn editor_view(&mut self, ui: &mut egui::Ui, factorio: &FactorioContext) -> bool {
        let label = ui.add(egui::text_edit::TextEdit::singleline(&mut self.name));
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

        egui::SidePanel::new(
            egui::containers::panel::Side::Left,
            egui::Id::new("boundary"),
        )
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

                self.summary_panel(ui, factorio, &mut changed, &mut need_suggestions);
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

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ProjectInstance {
    /// 存储游戏逻辑数据的全部上下文
    /// 包含一个 `Arc<DataContext>`，只读的游戏原型上下文
    /// 另外包含 UserContext，用户的自定义偏好
    pub factorio: FactorioContext,

    pub name: String,

    pub factories: IndexedVec<FactoryInstance>,

    #[serde(skip)]
    pub saved: bool,
    #[serde(skip)]
    pub file_path: Option<PathBuf>,
    #[serde(skip)]
    pub selected_page: ProjectPage,

    #[serde(skip)]
    pub factory_receiver: Receiver<FactoryInstance>,
    #[serde(skip)]
    pub factory_sender: Sender<FactoryInstance>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectPage {
    Index(usize), // 工厂设置页面
    #[default]
    UserContext, // 偏好设置页面
}

impl Default for ProjectInstance {
    fn default() -> Self {
        let (factory_tx, factory_rx) = channel();
        ProjectInstance {
            factorio: FactorioContext {
                data: Arc::new(DataContext::default()),
                user: UserContext::default(),
            },
            name: "未命名项目".to_string(),
            saved: true,
            file_path: None,
            factories: IndexedVec::new(),
            selected_page: ProjectPage::default(),
            factory_receiver: factory_rx,
            factory_sender: factory_tx,
        }
    }
}

impl ProjectInstance {
    pub fn new(data: DataContext) -> Self {
        ProjectInstance {
            factorio: FactorioContext {
                data: Arc::new(data.build_order_info().build_dependency_graph()),
                user: UserContext::default(),
            },
            ..Default::default()
        }
    }

    pub fn new_arc(data: Arc<DataContext>) -> Self {
        ProjectInstance {
            factorio: FactorioContext {
                data,
                user: UserContext::default(),
            },
            ..Default::default()
        }
    }

    pub fn set_data(&mut self, data: Arc<DataContext>) {
        self.factorio.data = data;
    }
}

impl SubView for ProjectInstance {
    fn view(&mut self, ui: &mut egui::Ui) {
        while let Ok(new_factory) = self.factory_receiver.try_recv() {
            self.factories.push(new_factory);
            self.saved = false;
        }
        ui.add(egui::text_edit::TextEdit::singleline(&mut self.name));
        ui.separator();
        egui::Frame::group(ui.style())
            .corner_radius(8.0)
            .stroke(egui::Stroke::new(
                1.0,
                ui.visuals().widgets.noninteractive.fg_stroke.color,
            ))
            .show(ui, |ui| {
                egui::containers::menu::MenuBar::new().ui(ui, |ui: &mut egui::Ui| {
                    ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
                    egui::ScrollArea::horizontal()
                        .id_salt("factories_button")
                        .show(ui, |ui| {
                            if ui.button("⚙ 偏好设置").clicked() {
                                self.selected_page = ProjectPage::UserContext;
                            }
                            if ui.button("+ 新建工厂").clicked() {
                                let name = "新工厂".to_string();
                                self.factories.push(
                                    FactoryInstance::new(name)
                                        .with_mechanic(RecipeMechanic::default())
                                        .with_mechanic(MiningMechanic::default())
                                        .with_sender(self.factory_sender.clone()),
                                );
                            }
                            ui.separator();
                            self.factories.dnd(
                                ui,
                                "factories",
                                |ui, real_idx, factory, handle, _, op| {
                                    ui.horizontal(|ui| {
                                        handle.ui(ui, |ui| {
                                            ui.label("≡");
                                        });
                                        let button =
                                            ui.add(egui::Button::new(&factory.name).selected(
                                                self.selected_page == ProjectPage::Index(real_idx),
                                            ));
                                        if button.clicked() {
                                            self.selected_page = ProjectPage::Index(real_idx);
                                        }
                                        if ui.button("×").clicked() {
                                            *op = EntryOpRequest::Drop;
                                            if let ProjectPage::Index(page) = self.selected_page
                                                && page >= real_idx
                                                && page > 0
                                            {
                                                self.selected_page = ProjectPage::Index(page - 1);
                                            }
                                        }
                                    });
                                },
                            );
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
                    egui::RichText::new("点击上方的新建工厂按钮创建一个新工厂。").append_to(
                        &mut layout_job,
                        ui.style(),
                        egui::FontSelection::Default,
                        egui::Align::Center,
                    );
                    ui.add_sized(ui.available_size(), egui::Label::new(layout_job));
                } else {
                    match self.selected_page {
                        ProjectPage::UserContext => {
                            self.saved &=
                                !ui.add(UserContextEditor::new(&mut self.factorio)).changed();
                        }
                        ProjectPage::Index(page) => {
                            if page >= self.factories.len() {
                                self.selected_page = ProjectPage::Index(0);
                            }
                            self.saved &= !self.factories.vec[page].editor_view(ui, &self.factorio);
                        }
                    }
                }
            });
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn description(&self) -> String {
        self.factorio.data.mods.iter().fold(
            "使用以下模组: ".to_string(),
            |mut acc, (mod_name, mod_version)| {
                acc.push_str(&format!("\n{} ({}), ", mod_name, mod_version));
                acc
            },
        )
    }
}

pub struct ProjectView {
    pub data: Arc<DataContext>,
    pub selected: Option<usize>,
    pub projects: IndexedVec<ProjectInstance>,
    pub ignore_close: bool,
    pub delete_request: DeleteRequest,
}

impl ProjectView {
    pub fn new(data: DataContext) -> Self {
        ProjectView {
            data: Arc::new(data.build_order_info().build_dependency_graph()),
            ignore_close: false,
            selected: None,
            projects: IndexedVec::new(),
            delete_request: DeleteRequest::None,
        }
    }
}

impl SubView for ProjectView {
    fn name(&self) -> String {
        "异星工厂规划器".to_string()
    }
    fn description(&self) -> String {
        format!(
            "使用的模组和版本：{}",
            self.data
                .mods
                .iter()
                .fold("".to_string(), |mut acc, (mod_name, mod_version)| {
                    acc.push_str(&format!("\n{} ({}), ", mod_name, mod_version));
                    acc
                },)
        )
    }
    fn view(&mut self, ui: &mut egui::Ui) {
        let mut show_close_confirm = false;
        if ui.input(|i| i.viewport().close_requested())
            && !self.ignore_close
            && self.projects.iter().any(|p| !p.saved)
        {
            show_close_confirm = true;
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }

        show_modal(egui::Id::new("关闭确认"), show_close_confirm, ui, |ui| {
            ui.label("确定要关闭程序吗？未保存的项目将会丢失。");
            ui.horizontal(|ui| {
                if ui.button("取消").clicked() {
                    ui.close();
                }
                if ui.button("关闭程序").clicked() {
                    self.ignore_close = true;
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("关闭前保存").clicked() {
                    for project in self.projects.vec.iter_mut() {
                        if !project.saved {
                            if let Some(path) = &project.file_path.clone() {
                                save_project(project, path);
                            } else {
                                save_project_as(project);
                            }
                            project.saved = true;
                        }
                    }
                    self.ignore_close = true;
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });

        egui::containers::menu::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("文件", |ui| {
                if ui.button("新建项目").clicked() {
                    self.projects
                        .push(ProjectInstance::new_arc(self.data.clone()));
                    self.selected = Some(self.projects.len() - 1);
                    ui.close();
                }
                if ui.button("加载项目").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("异星工厂规划项目文件", &["fpp"])
                        .set_title("打开项目文件")
                        .pick_file()
                        && let Some(mut project) = load_project(&path)
                    {
                        project.set_data(self.data.clone());
                        project.saved = true;
                        project.file_path = Some(path);
                        project.factories.vec.iter_mut().for_each(|f| {
                            f.send_solve_request(&project.factorio);
                            f.set_sender(project.factory_sender.clone());
                        });
                        self.projects.push(project);
                        self.selected = Some(self.projects.len() - 1);
                    }
                    ui.close();
                }
                if let Some(selected) = self.selected {
                    ui.separator();
                    let project = &mut self.projects[selected];
                    if ui.button("保存项目").clicked() {
                        if let Some(path) = &project.file_path.clone() {
                            save_project(project, path);
                        } else {
                            save_project_as(project);
                        }
                        ui.close();
                    }
                    if ui.button("另存为...").clicked() {
                        save_project_as(project);
                        ui.close();
                    }
                }
            })
        });
        ui.separator();
        let mut toggle = false;

        egui::containers::menu::MenuBar::new().ui(ui, |ui| {
            ui.label("项目列表");

            ui.separator();

            ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
            egui::ScrollArea::horizontal()
                .id_salt("projects")
                .show(ui, |ui| {
                    let mut virtual_idx = 0;
                    egui_dnd::dnd(ui, "files").show_vec(
                        &mut self.projects.idx,
                        |ui, real_idx, handle, _| {
                            ui.horizontal(|ui| {
                                handle.ui(ui, |ui| {
                                    ui.label("≡");
                                });
                                let project = &self.projects.vec[*real_idx];
                                let button = ui.add(
                                    egui::Button::new(&project.name)
                                        .selected(self.selected == Some(*real_idx)),
                                );
                                if button.clicked() {
                                    self.selected = Some(*real_idx);
                                }
                                if ui.button("×").clicked() {
                                    if !project.saved {
                                        toggle = true;
                                        self.delete_request = DeleteRequest::Pending(virtual_idx);
                                    } else {
                                        self.delete_request = DeleteRequest::Confirmed(virtual_idx);
                                    }
                                    ui.close();
                                }
                            });
                            virtual_idx += 1;
                        },
                    );
                });
        });
        ui.separator();
        if let Some(selected) = self.selected {
            self.projects.vec[selected].view(ui);
        }
        match self.delete_request {
            DeleteRequest::Pending(idx) => {
                show_modal(egui::Id::new("删除确认"), toggle, ui, |ui| {
                    ui.label("确定要删除该项目吗？此操作无法撤销。");
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            self.delete_request = DeleteRequest::None;
                            ui.close();
                        }
                        if ui.button("删除项目").clicked() {
                            self.delete_request = DeleteRequest::Confirmed(idx);
                            ui.close();
                        }
                    });
                });
            }
            DeleteRequest::Confirmed(idx) => {
                self.projects.remove(idx);
                self.selected = None;
                self.delete_request = DeleteRequest::None;
            }
            DeleteRequest::None => {}
        }
    }
}

pub enum DeleteRequest {
    None,
    Pending(usize),
    Confirmed(usize),
}

pub fn save_project_as(proj: &mut ProjectInstance) {
    let path = rfd::FileDialog::new()
        .add_filter("异星工厂规划项目文件", &["fpp"])
        .set_title("另存为项目文件")
        .set_file_name(format!("{}.fpp", proj.name))
        .save_file();
    if let Some(path) = path {
        save_project(proj, &path);
    }
}

pub fn save_project(proj: &mut ProjectInstance, path: &Path) {
    match std::fs::File::create(path) {
        Ok(file) => match serde_json::to_writer_pretty(&file, &proj) {
            Ok(_) => {
                proj.saved = true;
                proj.file_path = Some(path.to_path_buf());
                crate::toast::info("项目已保存");
            }
            Err(e) => {
                crate::toast::error(format!("保存项目失败: {:?}", e));
            }
        },
        Err(e) => {
            crate::toast::error(format!("创建文件失败: {:?}", e));
        }
    }
}

pub fn load_project(path: &Path) -> Option<ProjectInstance> {
    match std::fs::File::open(path) {
        Ok(file) => match serde_json::from_reader(BufReader::new(file)) {
            Ok(proj) => Some(proj),
            Err(e) => {
                crate::toast::error(format!("加载项目失败: {:?}", e));
                None
            }
        },
        Err(e) => {
            crate::toast::error(format!("打开文件失败: {:?}", e));
            None
        }
    }
}

#[derive(Default, Debug)]
pub struct FactorioContextCreatorView {
    path: Option<std::path::PathBuf>,
    mod_path: Option<std::path::PathBuf>,
    subview_sender: Option<Sender<Box<dyn SubView>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SubView for FactorioContextCreatorView {
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
                    ui.label(
            "若为 Steam 版本的游戏，请关闭正在运行中的异星工厂并且启动 Steam 再执行加载游戏上下文",
          );
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
                        move || match DataContext::load_from_executable_path(
                            &exe_path,
                            mod_path.as_deref(),
                            None,
                        ) {
                            Ok(data) => {
                                sender
                                    .send(Box::new(ProjectView::new(data)))
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
                        move || match DataContext::load_from_tmp_no_dump() {
                            Ok(data) => {
                                sender.send(Box::new(ProjectView::new(data))).unwrap();
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
    fn set_subview_sender(&mut self, sender: Sender<Box<dyn SubView>>) {
        self.subview_sender = Some(sender);
    }
}
