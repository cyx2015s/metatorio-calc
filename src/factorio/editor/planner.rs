use std::{
    collections::{HashMap, HashSet},
    io::BufReader,
    path::Path,
    sync::{Arc, mpsc::*},
};

use rayon::prelude::*;

use crate::{
    concept::*,
    factorio::{
        ProjectContext, ProjectPage,
        common::*,
        editor::{icon::*, modal::*},
        format::*,
        model::*,
        number::AmountLabel,
        selector::{Selector, generic_item_selector},
        setting::UserContextEditor,
        style::card_frame,
        update_accessibles,
    },
    math::*,
};

use indexmap::IndexMap;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FactoryContext {
    pub planet: Option<String>,

    // 自动和手动填充时，优先使用的机器的品质等级
    pub major_quality: u8,

    pub debug: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FactoryInstance {
    pub factory: FactoryContext,

    pub name: String,
    pub target: DndVec<(GenericItem, f64)>,
    pub external: DndVec<(GenericItem, f64)>,
    pub mechanics: Vec<Box<dyn FactorioMechanic>>,
    pub instances: Vec<(usize, usize)>,

    pub strict_source: bool,
    pub strict_sink: bool,
    #[serde(skip)]
    pub solution: SolverSolution<GenericItem, (usize, usize)>,
    #[serde(skip)]
    pub total_flow_sorted_keys: Vec<GenericItem>,
}

impl Default for FactoryInstance {
    fn default() -> Self {
        FactoryInstance {
            factory: FactoryContext::default(),

            name: "工厂".to_string(),
            target: DndVec::new(),
            external: DndVec::new(),
            mechanics: Vec::new(),
            instances: Vec::new(),

            strict_source: false,
            strict_sink: false,
            solution: SolverSolution::NotSolved {
                no_provider: vec![],
                no_consumer: vec![],
                description: "未求解".to_string(),
            },
            total_flow_sorted_keys: Vec::new(),
        }
        .with_mechanic(RecipeMechanic::default())
        .with_mechanic(MiningMechanic::default())
        .with_mechanic(ItemFuelMechanic::default())
        .with_mechanic(GeneratorMechanic::default())
        .with_mechanic(BoilerMechanic::default())
        .with_mechanic(ReactorMechanic::default())
        .with_mechanic(PlantMechanic::default())
        .with_mechanic(SpoilMechanic::default())
        .with_mechanic(FluidFuelMechanic::default())
        .with_mechanic(FluidHeatMechanic::default())
        .with_mechanic(ItemLaunchMechanic::default())
    }
}

impl FactoryInstance {
    pub fn new(name: String) -> Self {
        FactoryInstance {
            name,
            ..Default::default()
        }
    }

    pub fn with_mechanic(mut self, mechanic: impl FactorioMechanic) -> Self {
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

    pub fn as_problem(
        &mut self,
        data: &DataContext,
        proj: &ProjectContext,
    ) -> SolverData<GenericItem, (usize, usize)> {
        if self
            .mechanics
            .iter()
            .map(|m| m.instance_len())
            .sum::<usize>()
            != self.instances.len()
        {
            self.reset_instances();
        }

        let mut flows = self
            .instances
            .par_iter()
            .map(|(idx, jdx)| {
                let fe = &self.mechanics[*idx].instances()[*jdx];
                (
                    (*idx, *jdx),
                    (
                        fe.as_flow(data, proj, &self.factory),
                        fe.cost(data, proj, &self.factory),
                    ),
                )
            })
            .collect::<IndexMap<_, _>>();

        let target = self
            .target
            .iter()
            .map(|(item, amount)| (item.clone(), *amount))
            .fold(IndexMap::new(), |mut acc, (item, amount)| {
                *acc.entry(item).or_insert(0.0) +=
                    amount * (if item.is_energy() { 1e6 } else { 1.0 });
                acc
            });
        let mut external = self
            .external
            .iter()
            .map(|(item, penalty)| {
                (
                    item.clone(),
                    if item.is_energy() { 1e-6 } else { 1.0 } * penalty,
                )
            })
            .collect::<IndexMap<_, _>>();

        if let Some(planet_name) = &self.factory.planet
            && let Some(planet) = data.planets.get(planet_name)
        {
            let autoplaced = planet.collect_autoplaced(data);
            for item in &autoplaced {
                if !external.contains_key(item) && !target.contains_key(item) {
                    external.insert(item.clone(), 0.0);
                }
            }
            for pollutant in data.airborne_pollutants.keys() {
                let key = GenericItem::Pollution {
                    name: pollutant.clone(),
                };
                if !external.contains_key(&key) && !target.contains_key(&key) {
                    external.insert(key, 1.0);
                }
            }
        }

        let mut fluid_temperaturess = HashMap::new();
        let mut fluid_fuels = HashSet::new();
        let mut fluid_heats = HashSet::new();
        for (flow, _) in flows.values() {
            for (item, _) in flow {
                update_fluid_metainfo(
                    &mut fluid_temperaturess,
                    &mut fluid_fuels,
                    &mut fluid_heats,
                    item,
                );
            }
        }
        for (source, _) in &external {
            update_fluid_metainfo(
                &mut fluid_temperaturess,
                &mut fluid_fuels,
                &mut fluid_heats,
                source,
            );
        }
        for (target, _) in &target {
            update_fluid_metainfo(
                &mut fluid_temperaturess,
                &mut fluid_fuels,
                &mut fluid_heats,
                target,
            );
        }
        let mut aux_idx = 0;
        for (fluid, temperatures) in &fluid_temperaturess {
            // 添加将限定更严格的温度转换为更宽松的温度的流
            for narrow in temperatures {
                for broad in temperatures {
                    if narrow[0] >= broad[0] && narrow[1] <= broad[1] && narrow != broad {
                        let mut flow = Flow::new();
                        flow.insert(
                            GenericItem::Fluid {
                                name: fluid.clone(),
                                temperature: *narrow,
                            },
                            -1.0,
                        );
                        flow.insert(
                            GenericItem::Fluid {
                                name: fluid.clone(),
                                temperature: *broad,
                            },
                            1.0,
                        );
                        log::debug!("添加温度转换流 {}：{:?} -> {:?}", fluid, narrow, broad);
                        flows.insert((usize::MAX, aux_idx), (flow, 0.0));
                        aux_idx += 1;
                    }
                }
            }
        }
        fluid_fuels.into_iter().for_each(|fluid| {
            let mut flow = Flow::new();
            flow.insert(
                GenericItem::FluidFuel {
                    filter: fluid.into(),
                },
                -1.0,
            );
            flow.insert(GenericItem::FluidFuel { filter: None }, 1.0);
            // 燃料转换代价为 0
            flows.insert((usize::MAX, aux_idx), (flow, 0.0));
            aux_idx += 1;
        });
        fluid_heats.into_iter().for_each(|fluid| {
            let mut flow = Flow::new();
            flow.insert(
                GenericItem::FluidHeat {
                    filter: fluid.into(),
                },
                -1.0,
            );
            flow.insert(GenericItem::FluidHeat { filter: None }, 1.0);
            // 热量转换代价为 0
            flows.insert((usize::MAX, aux_idx), (flow, 0.0));
            aux_idx += 1;
        });
        let mut sinks = IndexMap::new();
        for pollutant in &data.airborne_pollutants {
            sinks.insert(
                crate::factorio::GenericItem::Pollution {
                    name: pollutant.0.clone(),
                },
                0.0,
            );
        }
        SolverData::new(target, flows)
            .with_sources(external)
            .with_strict_source(self.strict_source)
            .with_strict_sink(self.strict_sink)
            .with_sinks(sinks)
    }

    pub fn trim_flows(&mut self) -> bool {
        let mut prim_raw_log_sum = 0.0;
        let mut prim_raw_log_min = f64::INFINITY;
        let mut prim_raw_log_max = f64::NEG_INFINITY;
        let mut prim_raw_count = 0;
        for (idx, mechanic) in self.mechanics.iter_mut().enumerate() {
            for jdx in 0..mechanic.instance_len() {
                if let Some(cur_prim_raw) = self.solution.get_prim_raw_of(&(idx, jdx))
                    && cur_prim_raw > 0.0
                {
                    let cur_log = cur_prim_raw.log2().max(-1024.0);
                    prim_raw_log_sum += cur_log;
                    prim_raw_log_min = prim_raw_log_min.min(cur_log);
                    prim_raw_log_max = prim_raw_log_max.max(cur_log);
                    prim_raw_count += 1;
                }
            }
        }
        let prim_raw_log_avg = if prim_raw_count > 0 {
            prim_raw_log_sum / prim_raw_count as f64
        } else {
            0.0
        };
        log::debug!(
            "平均原始流量的 log2 值为 {:.2}, 约为 ({:e})",
            prim_raw_log_avg,
            2.0_f64.powf(prim_raw_log_avg)
        );
        log::debug!(
            "原始流量的 log2 值范围为 [{:.2}, {:.2}], 约为 [{:e}, {:e}]",
            prim_raw_log_min,
            prim_raw_log_max,
            2.0_f64.powf(prim_raw_log_min),
            2.0_f64.powf(prim_raw_log_max)
        );
        // let threshold = 2.0_f64.powf(prim_raw_log_avg - 6.0);
        let threshold = 2.0_f64
            .powf(
                (prim_raw_log_avg - 15.0)
                    .min(prim_raw_log_max - 30.0)
                    .min(prim_raw_log_min + 15.0)
                    .min(prim_raw_log_max - (prim_raw_log_max - prim_raw_log_avg) * 2.0),
            )
            .max(1e-12);
        let mut changed = false;
        self.mechanics
            .iter_mut()
            .enumerate()
            .for_each(|(idx, mechanic)| {
                for jdx in 0..mechanic.instance_len() {
                    mechanic.instance_operate(jdx, &mut |_| match self
                        .solution
                        .get_prim_raw_of(&(idx, jdx))
                    {
                        Some(n) => {
                            if n < threshold {
                                changed = true;
                                EntryOpRequest::Drop
                            } else {
                                EntryOpRequest::None
                            }
                        }
                        None => {
                            changed = true;
                            EntryOpRequest::Drop
                        }
                    });
                }
                mechanic.submit_operations();
            });

        changed
    }

    fn flows_panel(
        &mut self,
        ui: &mut egui::Ui,
        data: &DataContext,
        proj: &ProjectContext,
        changed: &mut bool,
        need_suggestions: &mut bool,
    ) {
        let mut display_idx = 0usize;
        egui_dnd::dnd(ui, "instances").show_vec(
            &mut self.instances,
            |ui, &mut (idx, jdx), handle, _| {
                display_idx += 1;
                let item_id = ui.make_persistent_id(("dnd_item", idx, jdx));
                // 从 egui 存储中获取上一帧的高度，默认为估算值 100.0
                let last_frame_height =
                    ui.data_mut(|d| d.get_temp::<f32>(item_id).unwrap_or(100.0));

                let clip_rect = ui.clip_rect(); // 当前可见屏幕范围
                let current_cursor = ui.cursor(); // 当前绘制光标位置

                let visible = !(current_cursor.min.y > clip_rect.max.y
                    || current_cursor.min.y + last_frame_height < clip_rect.min.y);
                let solution_value = self.solution.get_prim_of(&(idx, jdx));
                let solution_raw_value = self.solution.get_prim_raw_of(&(idx, jdx));
                let response = ui
                    .scope(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            card_frame(ui).show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        handle.ui(ui, |ui| {
                                            ui.heading("≡");
                                            ui.label(format!("#{display_idx:05}"));
                                        });
                                    });
                                    ui.horizontal(|ui| {
                                        let button =
                                            ui.add_sized([28.0, 14.0], egui::Button::new("⧉"));
                                        if button.clicked() {
                                            self.mechanics[idx].instance_operate(jdx, &mut |_| {
                                                EntryOpRequest::Clone
                                            });
                                        }
                                        let button =
                                            ui.add_sized([28.0, 14.0], egui::Button::new("🗑"));
                                        if button.clicked() {
                                            self.mechanics[idx].instance_operate(jdx, &mut |_| {
                                                EntryOpRequest::Drop
                                            });
                                        }
                                    });
                                    if let Some(value) = solution_value {
                                        ui.add(AmountLabel::new(value));
                                        if self.factory.debug {
                                            ui.add(AmountLabel::new(solution_raw_value.unwrap()));
                                        }
                                    } else {
                                        ui.label("无解");
                                    }
                                });
                            });
                            card_frame(ui).show(ui, |ui| {
                                let target_width = ui.available_width() * 0.3;
                                ui.set_min_width(target_width);
                                ui.set_max_width(target_width);
                                *changed |= self.mechanics[idx].instance_view(
                                    jdx,
                                    ui,
                                    data,
                                    proj,
                                    &self.factory,
                                );
                            });
                            card_frame(ui).show(ui, |ui| {
                                let target_width = ui.available_width();
                                ui.set_min_width(target_width);
                                ui.set_max_width(target_width);
                                ui.set_min_height(50.0);
                                if !visible {
                                    return;
                                }
                                let flow = self.mechanics[idx].instances()[jdx].as_flow(
                                    data,
                                    proj,
                                    &self.factory,
                                );

                                let mut flow_keys = flow.keys().cloned().collect::<Vec<_>>();
                                sort_generic_items_owned(&mut flow_keys, data);
                                // 先展示输入，再展示输出
                                for item in &flow_keys {
                                    let amount = flow.get(item).cloned().unwrap_or(0.0);
                                    if amount.abs() < 1e-8 && !self.factory.debug {
                                        continue;
                                    }

                                    ui.vertical(|ui| {
                                        ui.set_min_width(40.0);
                                        ui.set_max_width(40.0);
                                        let button = ui
                                            .add_sized([25.0, 25.0], GenericIcon::new(data, item))
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
                                                    mechanic.update_suggestion(
                                                        data,
                                                        proj,
                                                        &self.factory,
                                                        item,
                                                        amount,
                                                    )
                                                });
                                            }
                                        });
                                        if button.clicked() {
                                            *need_suggestions = true;
                                            self.mechanics.iter_mut().for_each(|mechanic| {
                                                mechanic.update_suggestion(
                                                    data,
                                                    proj,
                                                    &self.factory,
                                                    item,
                                                    amount,
                                                )
                                            });
                                        }

                                        ui.add(
                                            AmountLabel::new(
                                                amount * solution_value.unwrap_or(1.0),
                                            )
                                            .with_time_scale(proj.time_scale)
                                            .with_is_energy(item.is_energy())
                                            .with_is_signed(true),
                                        );
                                    });
                                    if ui.available_size_before_wrap().x < 35.0 {
                                        ui.end_row();
                                        ui.add_space(4.0);
                                    }
                                }
                                // });
                            })
                        });
                    })
                    .response;
                let actual_height = response.rect.height();
                ui.data_mut(|d| d.insert_temp(item_id, actual_height));
            },
        );
    }

    fn summary_panel(
        &mut self,
        ui: &mut egui::Ui,
        data: &DataContext,
        proj: &ProjectContext,
        changed: &mut bool,
        need_suggestions: &mut bool,
    ) {
        ui.horizontal(|ui| {
            *changed |= ui
                .checkbox(&mut self.strict_source, "禁止无端引入原料")
                .changed();
            *changed |= ui.checkbox(&mut self.strict_sink, "禁止副产物").changed();
            ui.checkbox(&mut self.factory.debug, "显示调试数据");
            if ui.button("删除无用配方").clicked() {
                *changed |= self.trim_flows();
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
            if ui.button("删除无解配方").clicked() {
                self.mechanics
                    .iter_mut()
                    .enumerate()
                    .for_each(|(idx, mechanic)| {
                        for jdx in 0..mechanic.instance_len() {
                            mechanic.instance_operate(jdx, &mut |_| match self
                                .solution
                                .get_prim_of(&(idx, jdx))
                            {
                                Some(_) => EntryOpRequest::None,
                                None => EntryOpRequest::Drop,
                            });
                        }
                        mechanic.submit_operations();
                    });
                if self
                    .mechanics
                    .iter()
                    .map(|m| m.instance_len())
                    .sum::<usize>()
                    != self.instances.len()
                {
                    *changed = true;
                    self.reset_instances();
                }
            }
            if ui.button("按比例排序").clicked() {
                self.instances.sort_by(|a, b| {
                    let prim_raw_a = self.solution.get_prim_raw_of(a).unwrap_or(0.0);
                    let prim_raw_b = self.solution.get_prim_raw_of(b).unwrap_or(0.0);
                    // 取负号使得流量大的排在前面
                    prim_raw_b
                        .partial_cmp(&prim_raw_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            if ui
                .button("\u{26A0}自动规划")
                .on_hover_text("\u{26A0}新工厂会出现在一个新页面中")
                .clicked()
            {
                let data_cloned = data.clone();
                let proj_cloned = proj.clone();
                let factory_cloned = self.clone();
                std::thread::spawn(move || {
                    let sender = proj_cloned.factory_sender.clone();

                    let auto_planned_factory =
                        factorio_auto_planner(factory_cloned, data_cloned, proj_cloned);
                    match auto_planned_factory {
                        Ok(factory) => {
                            sender.unwrap().send(factory).unwrap();
                            crate::toast::info("自动规划工厂已添加到项目中。");
                        }
                        Err(e) => {
                            // crate::toast::error(format!("自动规划工厂失败：{:?}\n", &e));
                            log::error!("自动规划工厂失败: {:?}", &e);
                        }
                    }
                });
            }
        });
        ui.label(format!(
            "总代价: {:.2} | 总物料流",
            self.solution.get_cost().unwrap_or(f64::NAN)
        ));
        egui::ScrollArea::vertical().id_salt(4).show(ui, |ui| {
            ui.set_max_height(200.0);
            ui.horizontal_wrapped(|ui| {
                card_frame(ui).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.set_min_height(50.0);

                    for item in &self.total_flow_sorted_keys {
                        let raw_amount = self.solution.get_sum_raw_of(item).unwrap_or(0.0);

                        if raw_amount.abs() < 1e-12 {
                            continue;
                        }
                        let amount = self.solution.get_sum_of(item).unwrap_or(0.0);

                        ui.vertical(|ui| {
                            ui.set_min_width(40.0);
                            ui.add_sized(
                                [40.0, 15.0],
                                AmountLabel::new(amount)
                                    .with_time_scale(proj.time_scale)
                                    .with_is_energy(item.is_energy())
                                    .with_is_signed(true),
                            );
                            if self.factory.debug {
                                ui.add_sized(
                                    [40.0, 15.0],
                                    AmountLabel::new(raw_amount)
                                        .with_time_scale(proj.time_scale)
                                        .with_is_signed(true),
                                );
                            }
                            ui.push_id(item, |ui| {
                                let button = ui
                                    .add_sized([35.0, 35.0], GenericIcon::new(data, item))
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
                                            mechanic.update_suggestion(
                                                data,
                                                proj,
                                                &self.factory,
                                                item,
                                                amount,
                                            )
                                        });
                                    }
                                });
                                if button.clicked() {
                                    *need_suggestions = true;
                                    self.mechanics.iter_mut().for_each(|mechanic| {
                                        mechanic.update_suggestion(
                                            data,
                                            proj,
                                            &self.factory,
                                            item,
                                            amount,
                                        )
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
        });
    }

    fn side_panel(
        &mut self,
        ui: &mut egui::Ui,
        data: &DataContext,
        proj: &ProjectContext,
        changed: &mut bool,
        need_suggestions: &mut bool,
    ) {
        ui.scope(|ui| {
            self.target_editor(ui, data, proj, changed, need_suggestions);
        });

        ui.separator();
        ui.scope(|ui| {
            self.external_editor(ui, data, proj, changed, need_suggestions);
        });
        ui.separator();
        ui.heading("环境");
        ui.horizontal_wrapped(|ui| {
            let mut planet_name = self.factory.planet.clone();

            egui::ComboBox::from_label("星球")
                .selected_text(
                    planet_name
                        .as_ref()
                        .map(|name| data.get_display_name("space-location", name))
                        .unwrap_or("无".into()),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut planet_name, None, "无");
                    for name in data.planets.keys() {
                        ui.selectable_value(
                            &mut planet_name,
                            Some(name.clone()),
                            data.get_display_name("space-location", name),
                        );
                    }
                });
            self.factory.planet = planet_name;
        });
        ui.horizontal_wrapped(|ui| {
            let button = ui
                .add_sized(
                    [35.0, 35.0],
                    Icon::new(
                        data,
                        "quality",
                        &data.qualities[self.factory.major_quality as usize]
                            .base
                            .name,
                    ),
                )
                .interact(egui::Sense::click());
            ui.label("优先使用的机器品质");
            let mut quality: Option<String> = None;
            ui.add(
                SelectorModal::new(button.id, data, "选择偏好品质")
                    .with_toggle(button.clicked())
                    .with_selector(Selector::new(data, "quality").with_output(&mut quality)),
            );
            if let Some(quality_name) = quality {
                for (idx, q) in data.qualities.iter().enumerate() {
                    if q.base.name == quality_name {
                        self.factory.major_quality = idx as u8;
                        break;
                    }
                }
            }
            if self.factory.major_quality > (data.qualities.len() - 1) as u8 {
                self.factory.major_quality = (data.qualities.len() - 1) as u8;
            }
        });
        ui.separator();
        ui.heading("游戏机制");
        for mechanic in self.mechanics.iter_mut() {
            ui.separator();
            *changed |= mechanic.editor_view(ui, data, proj, &self.factory);
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
        data: &DataContext,
        proj: &ProjectContext,
        changed: &mut bool,
        need_suggestions: &mut bool,
    ) {
        let data = &data;
        ui.heading("额外输入代价");
        ui.label("对物品和流体而言，每秒产出1个所消耗的地格；对能量而言，产出1MW所消耗的地格");
        self.external
            .dnd(ui, "external", |ui, _, (item, penalty), handle, _, op| {
                card_frame(ui).show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.set_min_width(ui.available_width());

                        handle.ui(ui, |ui| {
                            ui.heading("≡");
                        });

                        *changed |= ui.add(drag_value(penalty)).changed();

                        if ui.button("×").clicked() {
                            *op = EntryOpRequest::Drop;
                            *changed = true;
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        let icon = ui
                            .add_sized([35.0, 35.0], GenericIcon::new(data, item))
                            .interact(egui::Sense::click());
                        if icon.clicked_by(egui::PointerButton::Secondary) {
                            *need_suggestions = true;
                            self.mechanics.iter_mut().for_each(|mechanic| {
                                mechanic.update_suggestion(
                                    data,
                                    proj,
                                    &self.factory,
                                    item,
                                    1.0, // 尝试消耗更多该物品
                                )
                            });
                        }
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                *changed |= generic_item_selector(
                                    ui,
                                    data,
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
            for planet in data.planets.values() {
                if ui
                    .button(data.get_display_name("space-location", &planet.base.name))
                    .clicked()
                {
                    self.external.clear();
                    let available = planet.collect_autoplaced(data);
                    for item in &available {
                        self.external.push((item.clone(), 0.0));
                    }
                    for pollution in data.airborne_pollutants.keys() {
                        self.external.push((
                            GenericItem::Pollution {
                                name: pollution.clone(),
                            },
                            0.0,
                        ));
                    }
                    *changed = true;
                }
            }
        });
    }

    fn target_editor(
        &mut self,
        ui: &mut egui::Ui,
        data: &DataContext,
        proj: &ProjectContext,
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
                            let mut display_value = *amount * 1e6;
                            *changed |= ui.add(drag_watt(&mut display_value).speed(1e6)).changed();
                            *amount = display_value / 1e6;
                        } else {
                            let mut display_value = *amount * proj.time_scale.multiplier();
                            *changed |= ui.add(drag_value(&mut display_value)).changed();
                            *amount = display_value / proj.time_scale.multiplier();
                        }

                        if ui.button("×").clicked() {
                            *op = EntryOpRequest::Drop;
                            *changed = true;
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        let mut icon = GenericIcon::new(data, item);
                        let solution_of_target = self.solution.get_sum_of(item).unwrap_or(0.0);
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
                                    data,
                                    proj,
                                    &self.factory,
                                    item,
                                    -*amount, // 目标产量为正表示目前缺少对应数量的物品
                                )
                            });
                        }
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                *changed |= generic_item_selector(
                                    ui,
                                    data,
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

fn update_fluid_metainfo(
    fluid_temperaturess: &mut HashMap<String, HashSet<[i32; 2]>>,
    fluid_fuels: &mut HashSet<String>,
    fluid_heats: &mut HashSet<String>,
    item: &GenericItem,
) {
    match item {
        GenericItem::Fluid { name, temperature } => {
            fluid_temperaturess
                .entry(name.clone())
                .or_default()
                .insert(*temperature);
        }
        GenericItem::FluidFuel {
            filter: Some(filter),
        } => {
            fluid_fuels.insert(filter.clone());
        }
        GenericItem::FluidHeat {
            filter: Some(filter),
        } => {
            fluid_heats.insert(filter.clone());
        }
        _ => {}
    }
}

impl SolveContext for FactoryInstance {
    type Game = DataContext;
    type Item = GenericItem;
}

impl FactoryInstance {
    fn view(&mut self, ui: &mut egui::Ui, data: &DataContext, proj: &ProjectContext) -> bool {
        ui.add(egui::text_edit::TextEdit::singleline(&mut self.name));
        ui.separator();
        let mut changed = false;
        let mut need_suggestions = false;

        egui::SidePanel::new(
            egui::containers::panel::Side::Left,
            egui::Id::new("boundary"),
        )
        .show_separator_line(true)
        .min_width(196.0)
        .max_width(196.0)
        .frame(egui::Frame::NONE.corner_radius(8.0).inner_margin(4.0))
        .show_inside(ui, |ui: &mut egui::Ui| {
            egui::ScrollArea::vertical().id_salt(1).show(ui, |ui| {
                self.side_panel(ui, data, proj, &mut changed, &mut need_suggestions);
            });
        });

        egui::Frame::NONE
            .corner_radius(8.0)
            .outer_margin(4.0)
            .show(ui, |ui| {
                ui.heading("配方配置");
                egui::ScrollArea::vertical().id_salt(2).show(ui, |ui| {
                    ui.set_max_height(150.0);
                    self.summary_panel(ui, data, proj, &mut changed, &mut need_suggestions);
                });
                egui::ScrollArea::vertical().id_salt(3).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
                        self.flows_panel(ui, data, proj, &mut changed, &mut need_suggestions);
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
                        changed |= mechanic.suggestion_view(ui, data, proj, &self.factory);
                        ui.separator();
                    });
                });
            });
        });
        // 无关
        changed
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ProjectInstance {
    #[serde(skip)]
    pub data: Arc<DataContext>,

    pub proj: ProjectContext,

    pub name: String,

    pub factories: DndVec<FactoryInstance>,

    #[serde(skip)]
    pub factory_receiver: Receiver<FactoryInstance>,

    #[serde(skip)]
    pub problem_sender: Sender<(usize, SolverData<GenericItem, (usize, usize)>)>,
    #[serde(skip)]
    pub solution_receiver: Receiver<(usize, SolverSolution<GenericItem, (usize, usize)>)>,
}

impl Default for ProjectInstance {
    fn default() -> Self {
        let (factory_tx, factory_rx) = channel();
        let (problem_tx, problem_rx) = channel();
        let (solution_tx, solution_rx) = channel();
        SolverData::make_solver_thread(solution_tx, problem_rx);
        ProjectInstance {
            data: Arc::new(DataContext::default()),
            proj: ProjectContext::default().with_factory_sender(factory_tx),

            name: "未命名项目".to_string(),
            factories: DndVec::new(),
            factory_receiver: factory_rx,
            problem_sender: problem_tx,
            solution_receiver: solution_rx,
        }
    }
}

impl ProjectInstance {
    pub fn new(data: DataContext) -> Self {
        let (factory_tx, factory_rx) = channel();
        log::debug!("ProjectInstance::new() called.");
        ProjectInstance {
            data: Arc::new(data.build_utility_info()),
            proj: ProjectContext::default().with_factory_sender(factory_tx),
            factory_receiver: factory_rx,
            ..Default::default()
        }
    }

    pub fn new_arc(data: Arc<DataContext>) -> Self {
        let (factory_tx, factory_rx) = channel();
        log::debug!("ProjectInstance::new_arc() called.");
        ProjectInstance {
            data,
            proj: ProjectContext::default().with_factory_sender(factory_tx),
            factory_receiver: factory_rx,
            ..Default::default()
        }
    }

    pub fn set_data(&mut self, data: Arc<DataContext>) {
        self.data = data;
    }

    pub fn with_default_milestones(mut self) -> Self {
        for (tech_name, tech) in &self.data.technologies {
            let mut is_essential = false;
            for effect in &tech.effects {
                match effect {
                    Modifier::UnlockQuality { .. } => {
                        is_essential = true;
                    }
                    Modifier::UnlockSpaceLocation { .. } => {
                        is_essential = true;
                    }
                    Modifier::UnlockRecipe { recipe } => {
                        if let Some(recipe) = self.data.recipes.get(recipe) {
                            for result in &recipe.results {
                                if let RecipeResult::Item(item) = result
                                    && let Some(item) = self.data.items.get(&item.name)
                                {
                                    if item.base.r#type == "tool" {
                                        is_essential = true;
                                        break;
                                    }
                                    if let Some(place_result) = &item.place_result
                                        && let Some(entity) = self.data.entities.get(place_result)
                                        && entity.base.r#type == "rocket-silo"
                                    {
                                        is_essential = true;
                                        break;
                                    }
                                    for launch_result in &item.rocket_launch_products {
                                        if let Some(launch_result) =
                                            self.data.items.get(&launch_result.name)
                                            && launch_result.base.r#type == "tool"
                                        {
                                            is_essential = true;
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            if is_essential {
                self.proj.tech_milestones.push((tech_name.clone(), true));
            }
        }

        update_accessibles(&mut self.proj, &self.data);
        self
    }

    pub fn reset_factory_channel(&mut self) {
        let (factory_tx, factory_rx) = channel();
        self.proj.factory_sender = Some(factory_tx);
        self.factory_receiver = factory_rx;
    }
}

impl SubView for ProjectInstance {
    fn view(&mut self, ui: &mut egui::Ui) {
        while let Ok(new_factory) = self.factory_receiver.try_recv() {
            let new_idx = self.factories.len();
            self.factories.push(new_factory);
            self.problem_sender
                .send((
                    new_idx,
                    self.factories.vec[new_idx].as_problem(&self.data, &self.proj),
                ))
                .unwrap();
            self.proj.saved = false;
        }
        while let Ok((req_id, result)) = self.solution_receiver.try_recv() {
            if req_id >= self.factories.len() {
                log::error!("棍母求解");
                continue;
            }
            let factory = &mut self.factories.vec[req_id];
            match result {
                SolverSolution::Solved { ref sum, .. } => {
                    // Update sorted keys cache when total_flow changes
                    factory.total_flow_sorted_keys = sum.keys().cloned().collect::<Vec<_>>();
                    factory.solution = result;

                    sort_generic_items_owned(&mut factory.total_flow_sorted_keys, &self.data);
                }
                SolverSolution::NotSolved { .. } => {
                    factory.total_flow_sorted_keys.clear();
                    factory.solution = result;
                }
            }
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
                                self.proj.selected_page = ProjectPage::UserContext;
                            }
                            if ui.button("+ 新建工厂").clicked() {
                                let name = "新工厂".to_string();
                                self.factories.push(FactoryInstance::new(name));
                            }
                            ui.separator();
                            self.factories.dnd(
                                ui,
                                "factories",
                                |ui, real_idx, factory, handle, _, op| {
                                    ui.horizontal(|ui| {
                                        egui::Frame::NONE
                                            .fill(
                                                if self.proj.selected_page
                                                    == ProjectPage::Index(real_idx)
                                                {
                                                    ui.visuals().selection.bg_fill
                                                } else {
                                                    ui.visuals().widgets.noninteractive.bg_fill
                                                },
                                            )
                                            .corner_radius(4.0)
                                            .show(ui, |ui| {
                                                if handle
                                                    .ui(ui, |ui| {
                                                        ui.label(&factory.name);
                                                    })
                                                    .interact(egui::Sense::click())
                                                    .clicked()
                                                {
                                                    self.proj.selected_page =
                                                        ProjectPage::Index(real_idx);
                                                }
                                                if ui.button("⧉").clicked() {
                                                    self.proj
                                                        .factory_sender
                                                        .as_ref()
                                                        .unwrap()
                                                        .send(factory.clone())
                                                        .unwrap();
                                                }
                                                if ui.button("×").clicked() {
                                                    *op = EntryOpRequest::Drop;
                                                    if let ProjectPage::Index(page) =
                                                        self.proj.selected_page
                                                        && page >= real_idx
                                                        && page > 0
                                                    {
                                                        self.proj.selected_page =
                                                            ProjectPage::Index(page - 1);
                                                    }
                                                }
                                            });
                                    });
                                },
                            );
                        });
                });
                ui.separator();

                match self.proj.selected_page {
                    ProjectPage::UserContext => {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
                            ui.heading("偏好设置");
                            ui.separator();
                            self.proj.saved &= !ui
                                .add(UserContextEditor::new(&self.data, &mut self.proj))
                                .changed();
                        });
                    }
                    ProjectPage::Index(page) => {
                        if self.factories.is_empty() {
                            let mut layout_job = egui::text::LayoutJob::default();
                            egui::RichText::new("没有工厂\n").size(32.0).append_to(
                                &mut layout_job,
                                ui.style(),
                                egui::FontSelection::Default,
                                egui::Align::Center,
                            );
                            egui::RichText::new("点击上方的新建工厂按钮创建一个新工厂。")
                                .append_to(
                                    &mut layout_job,
                                    ui.style(),
                                    egui::FontSelection::Default,
                                    egui::Align::Center,
                                );
                            ui.add_sized(ui.available_size(), egui::Label::new(layout_job));
                        } else {
                            if page >= self.factories.len() {
                                self.proj.selected_page = ProjectPage::Index(0);
                            }
                            if self.factories.vec[page].view(ui, &self.data, &self.proj) {
                                self.proj.saved = false;
                                self.problem_sender
                                    .send((
                                        page,
                                        self.factories.vec[page].as_problem(&self.data, &self.proj),
                                    ))
                                    .unwrap();
                            }
                        }
                    }
                }
            });
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn description(&self) -> String {
        self.data.mods.iter().fold(
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
    pub projects: DndVec<ProjectInstance>,
    pub ignore_close: bool,
    pub delete_request: DeleteRequest,
}

impl ProjectView {
    pub fn new(data: DataContext) -> Self {
        ProjectView {
            data: Arc::new(data.build_utility_info()),
            ignore_close: false,
            selected: None,
            projects: DndVec::new(),
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
            && self.projects.iter().any(|p| !p.proj.saved)
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
                        if !project.proj.saved {
                            if let Some(path) = &project.proj.file_path.clone() {
                                save_project(project, path);
                            } else {
                                save_project_as(project);
                            }
                            project.proj.saved = true;
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
                    self.projects.push(
                        ProjectInstance::new_arc(self.data.clone()).with_default_milestones(),
                    );
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
                        let makeup_factory = FactoryInstance::default();
                        for factory in &mut project.factories.vec {
                            for mechanic in &makeup_factory.mechanics {
                                if !factory
                                    .mechanics
                                    .iter()
                                    .any(|m| m.typetag_name() == mechanic.typetag_name())
                                {
                                    factory.mechanics.push(mechanic.clone());
                                }
                            }
                        }
                        project.reset_factory_channel();
                        project.set_data(self.data.clone());
                        update_accessibles(&mut project.proj, &project.data);
                        project.proj.saved = true;
                        project.proj.file_path = Some(path);
                        project
                            .factories
                            .vec
                            .iter_mut()
                            .enumerate()
                            .for_each(|(idx, f)| {
                                let _ = project
                                    .problem_sender
                                    .send((idx, f.as_problem(&project.data, &project.proj)));
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
                        if let Some(path) = &project.proj.file_path.clone() {
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
                                    if !project.proj.saved {
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

        if ui.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::S,
            )
        }) && let Some(selected) = self.selected
        {
            let project = &mut self.projects.vec[selected];
            save_project_as(project);
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S))
            && let Some(selected) = self.selected
        {
            let project = &mut self.projects.vec[selected];
            if let Some(path) = &project.proj.file_path.clone() {
                save_project(project, path);
            } else {
                save_project_as(project);
            }
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
                proj.proj.saved = true;
                proj.proj.file_path = Some(path.to_path_buf());
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
pub struct ContextCreatorView {
    path: Option<std::path::PathBuf>,
    mod_path: Option<std::path::PathBuf>,
    subview_sender: Option<Sender<Box<dyn SubView>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SubView for ContextCreatorView {
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
                        "若为 Steam 版本的游戏，请关闭正在运行中\
的异星工厂并且启动 Steam 再执行加载游戏上下文",
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
            ui.separator();
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
                let lang = "zh-CN".to_string();

                let sender = sender.clone();
                self.thread =
                    Some(std::thread::spawn(
                        move || match DataContext::load_from_executable_path(
                            &exe_path,
                            mod_path.as_deref(),
                            Some(&lang),
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

impl GameContextCreatorView for ContextCreatorView {
    fn set_subview_sender(&mut self, sender: Sender<Box<dyn SubView>>) {
        self.subview_sender = Some(sender);
    }
}
