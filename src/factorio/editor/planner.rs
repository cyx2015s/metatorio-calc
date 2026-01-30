use crate::{
    concept::*,
    dyn_deserialize::*,
    factorio::{
        common::*,
        editor::{icon::*, modal::*},
        format::*,
        model::*,
        style::card_frame,
    },
    solver::*,
};

use indexmap::IndexMap;
use lazy_static::lazy_static;

lazy_static! {
    static ref MECHANIC_REGISTRY: DynDeserializeRegistry<FactorioMechanic> = {
        let mut registry = DynDeserializeRegistry::default();
        RecipeConfig::register(&mut registry);
        MiningConfig::register(&mut registry);
        registry
    };
    static ref MECHANIC_PROVIDER_REGISTRY: DynDeserializeRegistry<FactorioMechanicProvider> = {
        let mut registry = DynDeserializeRegistry::default();
        RecipeConfigProvider::register(&mut registry);
        MiningConfigProvider::register(&mut registry);
        registry
    };
}

pub struct FactoryInstance {
    pub name: String,
    pub target: Vec<(GenericItem, f64)>,
    pub external: Vec<(GenericItem, f64)>,
    pub solution: (Flow<usize>, f64),
    pub total_flow: Flow<GenericItem>,
    /// Cached sorted keys for total_flow to avoid sorting every frame
    pub total_flow_sorted_keys: Vec<GenericItem>,
    pub mechanic_providers: Vec<Box<FactorioMechanicProvider>>,
    pub mechanics: Vec<Box<FactorioMechanic>>,
    pub mechanic_suggestions: Vec<Box<FactorioMechanic>>,
    pub mechanic_receiver: std::sync::mpsc::Receiver<Box<FactorioMechanic>>,
    pub mechanic_sender: std::sync::mpsc::Sender<Box<FactorioMechanic>>,
    pub arg_sender: std::sync::mpsc::Sender<SolverArgs<GenericItem, usize>>,
    pub solution_receiver: std::sync::mpsc::Receiver<SolverSolution<usize>>,
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
            "mechanic_providers",
            &self.mechanic_providers,
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
        for mechanic in value["mechanics"].as_array().unwrap_or(&vec![]) {
            let mech = MECHANIC_REGISTRY
                .deserialize(mechanic.clone())
                .ok_or_else(|| serde::de::Error::custom("Failed to deserialize mechanic"))?;
            factory_instance.mechanics.push(mech);
        }
        for mechanic_provider in value["mechanic_providers"].as_array().unwrap_or(&vec![]) {
            let mech_provider = MECHANIC_PROVIDER_REGISTRY
                .deserialize(mechanic_provider.clone())
                .ok_or_else(|| {
                    serde::de::Error::custom("Failed to deserialize mechanic provider")
                })?;
            factory_instance.mechanic_providers.push(mech_provider);
        }
        Ok(factory_instance)
    }
}

impl Clone for FactoryInstance {
    fn clone(&self) -> Self {
        let (arg_tx, arg_rx) = std::sync::mpsc::channel();
        let (solution_tx, solution_rx) = std::sync::mpsc::channel();
        let (mechanic_tx, mechanic_rx) = std::sync::mpsc::channel();
        SolverData::make_solver_thread(solution_tx, arg_rx);

        FactoryInstance {
            name: self.name.clone(),
            target: self.target.clone(),
            external: self.external.clone(),
            solution: self.solution.clone(),
            total_flow: self.total_flow.clone(),
            total_flow_sorted_keys: self.total_flow_sorted_keys.clone(),
            mechanic_providers: self.mechanic_providers.clone(),
            mechanics: self.mechanics.clone(),
            mechanic_suggestions: self.mechanic_suggestions.clone(),
            mechanic_receiver: mechanic_rx,
            mechanic_sender: mechanic_tx,
            arg_sender: arg_tx,
            solution_receiver: solution_rx,
        }
    }
}

impl Default for FactoryInstance {
    fn default() -> Self {
        let (mechanic_tx, mechanic_rx) = std::sync::mpsc::channel();
        let (arg_tx, arg_rx) = std::sync::mpsc::channel();
        let (solution_tx, solution_rx) = std::sync::mpsc::channel();
        SolverData::make_solver_thread(solution_tx, arg_rx);

        FactoryInstance {
            name: "工厂".to_string(),
            target: Vec::new(),
            external: Vec::new(),
            solution: (IndexMap::new(), 0.0),
            total_flow: IndexMap::new(),
            total_flow_sorted_keys: Vec::new(),
            mechanic_providers: Vec::new(),
            mechanics: Vec::new(),
            mechanic_suggestions: Vec::new(),
            mechanic_receiver: mechanic_rx,
            mechanic_sender: mechanic_tx,
            arg_sender: arg_tx,
            solution_receiver: solution_rx,
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

    pub fn send_solve_request(&self, ctx: &FactorioContext) {
        let flows = self
            .mechanics
            .iter()
            .map(|fe| (box_as_ptr(fe), (fe.as_flow(ctx), fe.cost(ctx))))
            .collect::<IndexMap<usize, (_, _)>>();
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
                *acc.entry(item).or_insert(0.0) += amount;
                acc
            });
        let _ = self.arg_sender.send((target, flows, external));
    }

    pub fn add_flow_source<
        F: Fn(MechanicSender<GenericItem, FactorioContext>) -> Box<FactorioMechanicProvider>,
    >(
        mut self,
        f: F,
    ) -> Self {
        self.mechanic_providers
            .push(f(self.mechanic_sender.clone()));
        self
    }

    fn flows_panel(&mut self, ui: &mut egui::Ui, ctx: &FactorioContext, changed: &mut bool) {
        let label = ui.label(format!("总代价: {:.2} | 总物料流", self.solution.1));
        ui.horizontal_wrapped(|ui| {
            card_frame(ui).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(50.0);
                let mut modal = HintModal::new(
                    label.id,
                    ctx,
                    &self.mechanic_sender,
                    &mut self.mechanic_suggestions,
                    &self.mechanic_providers,
                );
                let mut final_clicked = None;
                for item in &self.total_flow_sorted_keys {
                    let amount = self.total_flow.get(item).cloned().unwrap_or(0.0);
                    if amount.abs() < 1e-6 {
                        continue;
                    }
                    ui.vertical(|ui| {
                        ui.add_sized([35.0, 15.0], SignedCompactLabel::new(amount));
                        let icon = ui
                            .push_id(item, |ui| {
                                ui.add_sized([35.0, 35.0], GenericIcon::new(ctx, item))
                                    .interact(egui::Sense::click())
                            })
                            .inner;

                        if icon.clicked_by(egui::PointerButton::Secondary) || icon.clicked() {
                            final_clicked = Some((item, amount));
                        }
                    });
                    if ui.available_size_before_wrap().x < 35.0 {
                        ui.end_row();
                    }
                }
                if let Some((item, amount)) = final_clicked {
                    modal = modal.with_update(true, item, amount);
                }
                ui.add(modal);
            });
        });
        ui.separator();
        self.mechanics.retain_mut(|flow_config| {
            let mut deleted = false;
            card_frame(ui).show(ui, {
                |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let ptr = box_as_ptr(flow_config);
                        let solution_val = self.solution.0.get(&ptr).cloned();

                        ui.vertical(|ui| {
                            if ui.button("删除").clicked() {
                                deleted = true;
                                *changed = true;
                            }
                            if ui.button("复制").clicked() {
                                let serialized = serde_json::to_value(&flow_config);
                                let deserialized =
                                    MECHANIC_REGISTRY.deserialize(serialized.unwrap());
                                if let Some(deserialized) = deserialized {
                                    self.mechanic_sender.send(deserialized).unwrap();
                                }
                                *changed = true;
                            }
                            if let Some(solution) = solution_val {
                                ui.add(CompactLabel::new(solution));
                            } else {
                                ui.label("待解");
                            }
                        });

                        ui.separator();
                        ui.vertical(|ui: &mut egui::Ui| {
                            *changed |= flow_config.editor_view(ui, ctx)
                        });

                        ui.separator();
                        let flow = flow_config.as_flow(ctx);
                        let mut keys = flow.keys().collect::<Vec<_>>();
                        sort_generic_items(&mut keys, ctx);
                        ui.horizontal_top(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                for item in keys {
                                    let amount = flow.get(item).cloned().unwrap_or(0.0);

                                    ui.vertical(|ui| {
                                        ui.add_sized(
                                            [35.0, 15.0],
                                            SignedCompactLabel::new(
                                                amount * solution_val.unwrap_or(1.0),
                                            ),
                                        );
                                        let icon = ui
                                            .add_sized([35.0, 35.0], GenericIcon::new(ctx, item))
                                            .interact(egui::Sense::click());
                                        let toggle =
                                            icon.clicked_by(egui::PointerButton::Secondary);
                                        ui.add(
                                            HintModal::new(
                                                icon.id,
                                                ctx,
                                                &self.mechanic_sender,
                                                &mut self.mechanic_suggestions,
                                                &self.mechanic_providers,
                                            )
                                            .with_update(toggle, item, amount),
                                        );
                                    });
                                    if ui.available_size_before_wrap().x < 35.0 {
                                        ui.end_row();
                                    }
                                }
                            });
                        });
                    })
                }
            });
            !deleted
        });
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
    pub ctx: FactorioContext,

    pub factories: Vec<StatefulFactoryInstance>,

    pub selected_factory: usize,
    pub new_factory_name: String,
}

impl SolveContext for FactoryInstance {
    type GameContext = FactorioContext;
    type ItemIdentType = GenericItem;
}

impl EditorView for FactoryInstance {
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &FactorioContext) -> bool {
        ui.add(
            egui::text_edit::TextEdit::singleline(&mut self.name).font(egui::TextStyle::Heading),
        );
        ui.separator();
        let id = ui.id();
        let mut changed = false;

        while let Ok(result) = self.solution_receiver.try_recv() {
            match result {
                Ok(solution) => {
                    self.total_flow.clear();
                    self.solution = solution;
                    for fe in self.mechanics.iter_mut() {
                        let var_value =
                            self.solution.0.get(&box_as_ptr(fe)).cloned().unwrap_or(0.0);
                        let flow = fe.as_flow(ctx);
                        self.total_flow = flow_add(&self.total_flow, &flow, var_value);
                    }
                    // Update sorted keys cache when total_flow changes
                    self.total_flow_sorted_keys = self.total_flow.keys().cloned().collect();
                    sort_generic_items_owned(&mut self.total_flow_sorted_keys, ctx);
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
        // let err_info = ui.memory(|mem| mem.data.get_temp::<String>(id));

        egui::SidePanel::new(egui::containers::panel::Side::Left, egui::Id::new("target"))
            .show_separator_line(true)
            .frame(egui::Frame::NONE.corner_radius(8.0).inner_margin(4.0))
            .show_inside(ui, |ui: &mut egui::Ui| {
                egui::ScrollArea::vertical().id_salt(1).show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.heading("优化目标");
                            self.target.retain_mut(|(item, amount)| {
                                let mut deleted = false;
                                card_frame(ui).show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.horizontal_wrapped(|ui| {
                                        let icon = ui
                                            .vertical(|ui| {
                                                let icon = ui
                                                    .add_sized(
                                                        [35.0, 35.0],
                                                        GenericIcon::new(ctx, item),
                                                    )
                                                    .interact(egui::Sense::click());
                                                if ui.button("删除").clicked() {
                                                    deleted = true;
                                                    changed = true;
                                                }
                                                icon
                                            })
                                            .inner;
                                        let toggle =
                                            icon.clicked_by(egui::PointerButton::Secondary);
                                        ui.add(
                                            HintModal::new(
                                                icon.id,
                                                ctx,
                                                &self.mechanic_sender,
                                                &mut self.mechanic_suggestions,
                                                &self.mechanic_providers,
                                            )
                                            .with_update(toggle, item, -*amount),
                                        );
                                        ui.vertical(|ui| {
                                            egui::ComboBox::new(icon.id, "")
                                                .selected_text(match item {
                                                    GenericItem::Item { .. } => "物品",
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
                                                        GenericItem::Item("item-unknown".into()),
                                                        "物品",
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
                                            ui.horizontal(|ui| {
                                                match item {
                                                    GenericItem::Item(item_with_quality) => {
                                                        ui.add(
                                                            ItemWithQualitySelectorModal::new(
                                                                icon.id.with("target-select-item"),
                                                                ctx,
                                                                "选择物品",
                                                                "item",
                                                            )
                                                            .with_toggle(icon.clicked())
                                                            .with_current(item_with_quality)
                                                            .notify_change(&mut changed),
                                                        );
                                                    }
                                                    GenericItem::Fluid {
                                                        name,
                                                        temperature: _,
                                                    } => {
                                                        ui.add(
                                                            ItemSelectorModal::new(
                                                                egui::Id::new(
                                                                    "target-selecte-fluid",
                                                                ),
                                                                ctx,
                                                                "选择流体",
                                                                "fluid",
                                                            )
                                                            .with_toggle(icon.clicked())
                                                            .with_current(name)
                                                            .notify_change(&mut changed),
                                                        );
                                                    }
                                                    _ => {}
                                                }
                                                if ui.vertical(|ui| {
                                                    ui.label("目标产量");
                                                    ui.add(
                                                        egui::DragValue::new(amount).suffix("/秒"),
                                                    )
                                                }).inner.changed() {
                                                    changed = true;
                                                }
                                            });
                                        });
                                    });
                                });
                                !deleted
                            });
                            if ui.button("添加目标产物").clicked() {
                                self.target
                                    .push((GenericItem::Item("item-unknown".into()), 1.0));
                                changed = true;
                            }
                        })
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.heading("额外输入");
                        self.external.retain_mut(|(item, penalty)| {
                            let mut deleted = false;
                            card_frame(ui).show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.horizontal_wrapped(|ui| {
                                    let mut icon = ui
                                        .vertical(|ui| {
                                            let icon = ui
                                                .add_sized(
                                                    [35.0, 35.0],
                                                    GenericIcon::new(ctx, item),
                                                )
                                                .interact(egui::Sense::click());
                                            if ui.button("删除").clicked() {
                                                deleted = true;
                                                changed = true;
                                            }
                                            icon
                                        })
                                        .inner;
                                    if let GenericItem::Entity(..) = item {
                                        icon = icon.on_hover_text("⚠️ 指完成机制所消耗的实体资源（主要是矿物），不包括为了完成机制所需要收集的组装机、采矿机、插件塔等。")
                                    }
                                    let toggle = icon.clicked_by(egui::PointerButton::Secondary);
                                    ui.add(
                                        HintModal::new(
                                            icon.id,
                                            ctx,
                                            &self.mechanic_sender,
                                            &mut self.mechanic_suggestions,
                                            &self.mechanic_providers,
                                        )
                                        .with_update(toggle, item, -*penalty),
                                    );
                                    ui.vertical(|ui| {
                                        egui::ComboBox::new(icon.id, "")
                                            .selected_text(match item {
                                                GenericItem::Item { .. } => "物品",
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
                                                    GenericItem::Item("item-unknown".into()),
                                                    "物品",
                                                );
                                                ui.selectable_value(
                                                    item,
                                                    GenericItem::Fluid {
                                                        name: "fluid-unknown".to_string(),
                                                        temperature: None,
                                                    },
                                                    "流体",
                                                );
                                                ui.selectable_value(
                                                    item,
                                                    GenericItem::Entity("entity-unknown".into()),
                                                    "实体",
                                                );
                                            });
                                        ui.horizontal(|ui| {
                                            match item {
                                                GenericItem::Item(item_with_quality) => {
                                                    ui.add(
                                                        ItemWithQualitySelectorModal::new(
                                                            icon.id.with("target-select-item"),
                                                            ctx,
                                                            "选择物品",
                                                            "item",
                                                        )
                                                        .with_toggle(icon.clicked())
                                                        .with_current(item_with_quality)
                                                        .notify_change(&mut changed),
                                                    );
                                                }
                                                GenericItem::Fluid {
                                                    name,
                                                    temperature: _,
                                                } => {
                                                    ui.add(
                                                        ItemSelectorModal::new(
                                                            egui::Id::new("target-selecte-fluid"),
                                                            ctx,
                                                            "选择流体",
                                                            "fluid",
                                                        )
                                                        .with_toggle(icon.clicked())
                                                        .with_current(name)
                                                        .notify_change(&mut changed),
                                                    );
                                                }
                                                GenericItem::Entity(entity_with_quality) => {
                                                    ui.add(
                                                        ItemWithQualitySelectorModal::new(
                                                            icon.id.with("target-select-entity"),
                                                            ctx,
                                                            "选择实体",
                                                            "entity",
                                                        )
                                                        .with_toggle(icon.clicked())
                                                        .with_current(entity_with_quality)
                                                        .notify_change(&mut changed),
                                                    );
                                                }
                                                _ => {}
                                            }
                                            if ui.vertical(|ui| {
                                                ui.label("单位价值");
                                                ui.add(egui::DragValue::new(penalty).suffix("·秒"))
                                            }).inner.changed() {
                                                changed = true;
                                            };
                                            if *penalty < 0.0 {
                                                *penalty = 0.0
                                            }
                                        });
                                    });
                                });
                            });
                            !deleted
                        });
                        if ui.button("添加外部输入").clicked() {
                            self.external
                                .push((GenericItem::Item("item-unknown".into()), 1.0));
                            changed = true;
                        }
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.heading("游戏机制");
                        for flow_source in &mut self.mechanic_providers {
                            changed |= flow_source.editor_view(ui, ctx);
                            ui.separator();
                        }
                    })
                });
            });

        while let Ok(flow_source) = self.mechanic_receiver.try_recv() {
            self.mechanics.push(flow_source);
            changed = true;
        }
        egui::Frame::NONE
            .corner_radius(8.0)
            .outer_margin(4.0)
            .show(ui, |ui| {
                ui.heading("配方配置");
                egui::ScrollArea::vertical().id_salt(3).show(ui, |ui| {
                    ui.vertical(|ui| {
                        // Use cached sorted keys instead of sorting every frame
                        self.flows_panel(ui, ctx, &mut changed);
                    })
                    .response
                });
            });
        // 无关
        if changed {
            self.send_solve_request(ctx);
        };
        changed
    }
}

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
            &(serde_json::from_str(RAW_JSON)).unwrap(),
        ))
    }
}

impl Subview for PlannerView {
    fn view(&mut self, ui: &mut egui::Ui) {
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
                                    .add_flow_source(|s| {
                                        Box::new(
                                            RecipeConfigProvider::new().with_mechanic_sender(s),
                                        )
                                    })
                                    .add_flow_source(|s| {
                                        Box::new(
                                            MiningConfigProvider::new().with_mechanic_sender(s),
                                        )
                                    })
                                    .into(),
                            );
                        }

                        if ui
                            .add_enabled(
                                !self.factories.is_empty(),
                                egui::Button::new("另存为选中工厂……"),
                            )
                            .clicked()
                        {
                            // TODO 保存到文件
                            // 保存不可能出错吧
                            if let Some(path) = rfd::FileDialog::new()
                                .set_file_name(format!(
                                    "{}.fpc",
                                    self.factories[self.selected_factory].factory.name
                                ))
                                .add_filter("异星工厂规划配置", &["fpc", "json"])
                                .save_file()
                            {
                                if std::fs::write(
                                    &path,
                                    serde_json::to_string_pretty(
                                        &self.factories[self.selected_factory].factory,
                                    )
                                    .unwrap(),
                                )
                                .is_ok()
                                {
                                    self.factories[self.selected_factory].saved = true;
                                    self.factories[self.selected_factory].file_path = Some(path);
                                }
                            }
                        }
                        if ui.button("从文件加载工厂……").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("异星工厂规划配置", &["fpc", "json"])
                                .pick_file()
                            {
                                if let Ok(content) = std::fs::read_to_string(&path)
                                    && let Ok(value) =
                                        serde_json::from_str::<serde_json::Value>(&content)
                                    && let Ok(factory) =
                                        serde_json::from_value::<FactoryInstance>(value)
                                {
                                    factory.send_solve_request(&self.ctx);
                                    self.factories.push(StatefulFactoryInstance {
                                        factory,
                                        saved: true,
                                        file_path: Some(path),
                                    });
                                }
                            }
                        }
                    });
                });
                ui.separator();
                egui::containers::menu::MenuBar::new().ui(ui, |ui| {
                    ui.horizontal(|ui| {
                        for i in 0..self.factories.len() {
                            let button = ui.add(
                                egui::Button::new(format!(
                                    "{} {}",
                                    &self.factories[i].factory.name,
                                    if self.factories[i].saved { "" } else { "*" }
                                ))
                                .selected(self.selected_factory == i),
                            );
                            if button.clicked() {
                                self.selected_factory = i;
                            }
                            button.context_menu(|ui| {
                                if ui.button("关闭").clicked() {
                                    self.factories.remove(i);
                                    if self.selected_factory >= i && self.selected_factory > 0 {
                                        self.selected_factory -= 1;
                                    }
                                    ui.close();
                                }
                                if let Some(file_path) = &self.factories[i].file_path {
                                    if ui
                                        .button(format!("关闭并保存到 {}", file_path.display()))
                                        .clicked()
                                    {
                                        if std::fs::write(
                                            file_path,
                                            serde_json::to_string_pretty(
                                                &self.factories[self.selected_factory].factory,
                                            )
                                            .unwrap()
                                            .as_bytes(),
                                        )
                                        .is_ok()
                                        {
                                            self.factories.remove(i);
                                            if self.selected_factory >= i
                                                && self.selected_factory > 0
                                            {
                                                self.selected_factory -= 1;
                                            }
                                        }
                                        ui.close();
                                    }
                                }
                            });
                        }
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
                    factory.saved &= !factory.factory.editor_view(ui, &self.ctx);
                }
            });
    }

    fn name(&self) -> String {
        "异星工厂 - 工厂规划器".to_string()
    }

    fn description(&self) -> String {
        self.ctx.mods.iter().fold(
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
