use std::{
    io::BufReader,
    path::Path,
    sync::{Arc, mpsc::*},
    time::Instant,
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
        resolve_milestone_graph,
        selector::{Selector, generic_item_selector},
        setting::UserContextEditor,
        style::card_frame,
        update_accessibles,
    },
    math::*,
};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FactoryContext {
    pub planet: Option<String>,

    pub surface: Option<String>,

    // 自动和手动填充时，优先使用的机器的品质等级
    pub major_quality: u8,

    pub debug: bool,
}

impl FactoryContext {
    pub fn get_current_surface_properties<'a>(
        &self,
        data: &'a DataContext,
    ) -> Option<&'a Dict<f64>> {
        if let Some(surface) = &self.surface
            && let Some(surface) = data.surfaces.get(surface)
        {
            Some(&surface.surface_properties)
        } else {
            if let Some(planet) = &self.planet
                && let Some(planet) = data.planets.get(planet)
                && planet.has_surface()
            {
                Some(&planet.surface_properties)
            } else {
                None
            }
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TargetSpecVec<I: ItemIdent> {
    pub constant: f64,
    pub coefficients: Vec<(I, f64)>,
}

impl<T: ItemIdent> From<TargetSpecVec<T>> for TargetSpec<T> {
    fn from(value: TargetSpecVec<T>) -> Self {
        TargetSpec {
            constant: value.constant,
            coefficients: value.coefficients.into_iter().collect(),
        }
    }
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FactoryInstance {
    pub factory: FactoryContext,

    pub name: String,
    pub target: DndVec<(DualVar, f64)>,
    pub target_group: DndVec<TargetSpecVec<DualVar>>,
    pub external: DndVec<(DualVar, f64)>,
    pub mechanics: Vec<Box<dyn SerdeFactorioMechanic>>,
    pub instances: Vec<(usize, usize)>,

    pub strict_source: bool,
    pub strict_sink: bool,
    #[serde(skip)]
    pub suggesting_mechanic: usize,

    #[serde(skip)]
    pub solution: SolverSolution<DualVar, (usize, usize)>,
    #[serde(skip)]
    pub total_flow_sorted_keys: Vec<DualVar>,
}

impl FactoryInstance {
    pub fn new(name: String) -> Self {
        FactoryInstance {
            name,
            ..Default::default()
        }
    }

    pub fn with_mechanic(mut self, mechanic: impl SerdeFactorioMechanic) -> Self {
        self.mechanics.push(Box::new(mechanic));
        self
    }

    pub fn with_default_mechanics(mut self) -> Self {
        self.mechanics.clear();
        self.with_mechanic(RecipeMechanic::default())
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
    ) -> SolverData<DualVar, (usize, usize)> {
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
            .collect::<AIndexMap<_, _>>();

        let target = self
            .target
            .iter()
            .map(|(item, amount)| (item.clone(), *amount))
            .fold(AIndexMap::default(), |mut acc, (item, amount)| {
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
            .collect::<AIndexMap<_, _>>();

        if let Some(planet_name) = &self.factory.planet
            && let Some(planet) = data.planets.get(planet_name)
            && self.factory.surface.is_none()
        {
            let autoplaced = planet.collect_autoplaced(data);
            for (item, cost) in &autoplaced {
                if !external.contains_key(item) && !target.contains_key(item) {
                    external.insert(item.clone(), *cost);
                }
            }
        }

        for pollutant in data.airborne_pollutants.keys() {
            let key = DualVar::Pollution {
                name: pollutant.clone(),
            };
            if !external.contains_key(&key) && !target.contains_key(&key) {
                external.insert(key, 1.0);
            }
        }

        let mut fluid_temperaturess = AIndexMap::default();
        let mut fluid_fuels = AIndexSet::default();
        let mut fluid_heats = AIndexSet::default();
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
                        let mut flow = Flow::default();
                        flow.insert(
                            DualVar::Fluid {
                                name: fluid.clone(),
                                temperature: *narrow,
                            },
                            -1.0,
                        );
                        flow.insert(
                            DualVar::Fluid {
                                name: fluid.clone(),
                                temperature: *broad,
                            },
                            1.0,
                        );
                        // log::debug!("添加温度转换流 {}：{:?} -> {:?}", fluid, narrow, broad);
                        flows.insert((usize::MAX, aux_idx), (flow, 0.0));
                        aux_idx += 1;
                    }
                }
            }
        }
        fluid_fuels.into_iter().for_each(|fluid| {
            let mut flow = Flow::default();
            flow.insert(
                DualVar::FluidFuel {
                    filter: fluid.into(),
                },
                -1.0,
            );
            flow.insert(DualVar::FluidFuel { filter: None }, 1.0);
            // 燃料转换代价为 0
            flows.insert((usize::MAX, aux_idx), (flow, 0.0));
            aux_idx += 1;
        });
        fluid_heats.into_iter().for_each(|fluid| {
            let mut flow = Flow::default();
            flow.insert(
                DualVar::FluidHeat {
                    filter: fluid.into(),
                },
                -1.0,
            );
            flow.insert(DualVar::FluidHeat { filter: None }, 1.0);
            // 热量转换代价为 0
            flows.insert((usize::MAX, aux_idx), (flow, 0.0));
            aux_idx += 1;
        });
        let mut sinks = AIndexMap::default();
        for pollutant in &data.airborne_pollutants {
            sinks.insert(
                crate::factorio::DualVar::Pollution {
                    name: pollutant.0.clone(),
                },
                0.0,
            );
        }
        let mut ret = SolverData::new_simple(target, flows)
            .with_sources(external)
            .with_strict_source(self.strict_source)
            // .with_strict_sink(self.strict_sink)
            // .with_sinks(sinks)
            ;

        ret.target.extend(
            self.target_group
                .vec
                .iter()
                .cloned()
                .map(TargetSpecVec::into),
        );

        ret
    }

    pub fn trim_flows(&mut self) -> bool {
        let threshold = 1e-12;
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
        if changed {
            self.solution = SolverSolution::default();
        }
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
                                        ui.label(t!("metatorio.no-solution").to_string());
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
                                flow_keys.sort_by(|ka, kb| {
                                    flow[ka].signum().partial_cmp(&flow[kb].signum()).unwrap()
                                });

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
                                            .add_sized([25.0, 25.0], GenericIcon::new(data, item));

                                        button.context_menu(|ui| {
                                            if ui
                                                .button(t!("metatorio.add-to-production-target"))
                                                .clicked()
                                            {
                                                self.target.push((item.clone(), 0.0));
                                                *changed = true;
                                            }
                                            if ui
                                                .button(t!("metatorio.add-to-external-input"))
                                                .clicked()
                                            {
                                                self.external.push((item.clone(), 1.0));
                                                *changed = true;
                                            }
                                            if ui
                                                .button(t!("metatorio.show-recommended-recipes"))
                                                .clicked()
                                            {
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
                .checkbox(&mut self.strict_source, t!("metatorio.strict-source"))
                .changed();
            // *changed |= ui.checkbox(&mut self.strict_sink, t!("metatorio.strict-sink")).changed();
            ui.checkbox(&mut self.factory.debug, t!("metatorio.debug"));
            if ui.button(t!("metatorio.remove-unused-recipes")).clicked() {
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
            if ui
                .button(t!("metatorio.remove-unsolvable-recipes"))
                .clicked()
            {
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
            if ui.button(t!("metatorio.sort-by-ratio")).clicked() {
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
                .button(t!("metatorio.auto-plan"))
                .on_hover_text(t!("metatorio.auto-plan-tooltip"))
                .clicked()
            {
                let data_cloned = data.clone();
                let proj_cloned = proj.clone();
                let factory_cloned = self.clone();
                std::thread::spawn(move || {
                    let sender = proj_cloned.factory_sender.clone();

                    let auto_planned_factory =
                        auto_planner(factory_cloned, data_cloned, proj_cloned);
                    match auto_planned_factory {
                        Ok(factory) => {
                            sender.unwrap().send(factory).unwrap();
                            crate::toast::info(t!("metatorio.auto-plan-success"));
                        }
                        Err(e) => {
                            log::error!("自动规划工厂失败: {:?}", &e);
                        }
                    }
                });
            }
        });
        ui.label(t!(
            "metatorio.total-cost",
            format!("{:.2}", self.solution.get_cost().unwrap_or(f64::NAN))
        ));
        egui::ScrollArea::vertical().id_salt(4).show(ui, |ui| {
            ui.set_max_height(200.0);
            ui.horizontal_wrapped(|ui| {
                card_frame(ui).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.set_min_height(50.0);

                    for item in &self.total_flow_sorted_keys {
                        let raw_amount = self.solution.get_sum_raw_of(item).unwrap_or(0.0);

                        if raw_amount.abs() < 1e-8 && !self.factory.debug {
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
                                let button =
                                    ui.add_sized([35.0, 35.0], GenericIcon::new(data, item));
                                button.context_menu(|ui| {
                                    if ui
                                        .button(t!("metatorio.add-to-production-target"))
                                        .clicked()
                                    {
                                        self.target.push((item.clone(), 0.0));
                                        *changed = true;
                                    }
                                    if ui.button(t!("metatorio.add-to-external-input")).clicked() {
                                        self.external.push((item.clone(), 1.0));
                                        *changed = true;
                                    }
                                    if ui
                                        .button(t!("metatorio.show-recommended-recipes"))
                                        .clicked()
                                    {
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
        ui.heading(t!("metatorio.environment").to_string());
        if data.surfaces.len() > 0 {
            ui.label(t!("metatorio.environment-warning").to_string());
        }
        ui.horizontal_wrapped(|ui| {
            let button = if let Some(planet) = &self.factory.planet {
                ui.add_sized([35.0, 35.0], Icon::new(data, "space-location", planet))
            } else {
                ui.add_sized([35.0, 35.0], Icon::new(data, "item", "unknown"))
            };
            ui.add(
                SelectorModal::new(
                    button.id,
                    t!("metatorio.select-planet").to_string().as_str(),
                )
                .with_toggle(button.clicked())
                .with_selector(
                    Selector::new(data, "space-location").with_output(&mut self.factory.planet),
                ),
            );
            if button.secondary_clicked() {
                self.factory.planet.take();
            }
            ui.label(t!("metatorio.planet").to_string());
        });
        if data.surfaces.len() > 0 {
            ui.horizontal_wrapped(|ui| {
                let button = if let Some(surface) = &self.factory.surface {
                    ui.add_sized([35.0, 35.0], Icon::new(data, "surface", surface))
                } else {
                    ui.add_sized([35.0, 35.0], Icon::new(data, "item", "unknown"))
                };
                ui.add(
                    SelectorModal::new(
                        button.id,
                        t!("metatorio.select-surface").to_string().as_str(),
                    )
                    .with_toggle(button.clicked())
                    .with_selector(
                        Selector::new(data, "surface").with_output(&mut self.factory.surface),
                    ),
                );
                if button.secondary_clicked() {
                    self.factory.surface.take();
                }
                ui.label(t!("metatorio.surface").to_string());
            });
        }
        ui.horizontal_wrapped(|ui| {
            let button = ui.add_sized(
                [35.0, 35.0],
                Icon::new(
                    data,
                    "quality",
                    &data.qualities[self.factory.major_quality as usize]
                        .base
                        .name,
                ),
            );
            ui.label(t!("metatorio.major-quality").to_string());
            let mut quality: Option<String> = None;
            ui.add(
                SelectorModal::new(
                    button.id,
                    t!("metatorio.select-quality").to_string().as_str(),
                )
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
        ui.heading(t!("metatorio.mechanics").to_string());
        for (idx, mechanic) in self.mechanics.iter_mut().enumerate() {
            ui.separator();
            ui.scope_builder(egui::UiBuilder::new().id_salt(idx), |ui| {
                *changed |= mechanic.editor_view(ui, data, proj, &self.factory);
            });
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
        ui.heading(t!("metatorio.external-input-cost").to_string());
        ui.label(t!("metatorio.external-input-cost-description").to_string());
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
                        let icon = ui.add_sized([35.0, 35.0], GenericIcon::new(data, item));
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
        if ui.button(t!("metatorio.add-external-input")).clicked() {
            self.external
                .push((DualVar::Item("item-unknown".into()), 1.0));
            *changed = true;
        }
        ui.menu_button(t!("metatorio.auto-select-from-locations"), |ui| {
            for planet in data.planets.values() {
                if planet.has_surface()
                    && ui
                        .button(data.get_display_name("space-location", &planet.base.name))
                        .clicked()
                {
                    self.external.clear();
                    let available = planet.collect_autoplaced(data);
                    for (item, &cost) in &available {
                        self.external.push((item.clone(), cost));
                    }
                    *changed = true;
                }
            }
            for surface in data.surfaces.values() {
                if ui
                    .button(data.get_display_name("surface", &surface.base.name))
                    .clicked()
                {
                    self.external.clear();
                    self.external.push((DualVar::Electricity, 1.0));
                    for entity in data.entities.values() {
                        if entity.base.r#type != "asteroid-chunk" {
                            continue;
                        }
                        if let Some(minable) = entity.minable.as_ref() {
                            if let Some(result) = &minable.result {
                                self.external
                                    .push((DualVar::Item(result.clone().into()), 1.0));
                            } else {
                                for res in &minable.results {
                                    match res {
                                        RecipeResult::Item(item) => {
                                            self.external.push((
                                                DualVar::Item(item.name.clone().into()),
                                                1.0,
                                            ));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
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
        ui.heading(t!("metatorio.production-target").to_string());

        ui.scope(|ui| {
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
                                *changed |=
                                    ui.add(drag_watt(&mut display_value).speed(1e6)).changed();
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
                            let icon = GenericIcon::new(data, item);

                            let widget = ui.add_sized([35.0, 35.0], icon);

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
            if ui.button(t!("metatorio.add-specific-target")).clicked() {
                self.target
                    .push((DualVar::Item("item-unknown".into()), 1.0));
                *changed = true;
            }
        });
        ui.separator();
        ui.scope(|ui| {
            self.target_group.dnd(
                ui,
                "target-group",
                |ui,
                 _,
                 TargetSpecVec {
                     constant,
                     coefficients,
                 },
                 handle,
                 _,
                 op| {
                    card_frame(ui).show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.set_min_width(ui.available_width());
                            handle.ui(ui, |ui| {
                                ui.heading("≡");
                                ui.label(t!("metatorio.target-expression"));
                            });

                            if ui.button("×").clicked() {
                                *op = EntryOpRequest::Drop;
                                *changed = true;
                            }
                        });
                        ui.separator();
                        ui.label(t!("metatorio.constant-term"));
                        *changed |= ui.add(drag_value(constant)).changed();
                        ui.separator();
                        ui.label(t!("metatorio.linear-terms"));
                        coefficients.retain_mut(|(item_id, coef)| {
                            let mut deleted = false;
                            ui.horizontal(|ui| {
                                let response =
                                    ui.add_sized([35.0, 35.0], GenericIcon::new(data, item_id));
                                *changed |= generic_item_selector(
                                    ui,
                                    data,
                                    item_id,
                                    &response,
                                    response.id,
                                );
                                ui.vertical(|ui| {
                                    *changed |= ui.add(drag_value(coef).prefix("× ")).changed();
                                    if ui.button(t!("metatorio.delete")).clicked() {
                                        deleted = true;
                                        *changed = true;
                                    }
                                })
                            });
                            !deleted
                        });
                        if ui.button(t!("metatorio.add-item")).clicked() {
                            coefficients.push((DualVar::Item("item-unknown".into()), 1.0));
                            *changed = true;
                        }
                    });
                },
            );
            if ui.button(t!("metatorio.add-target-expression")).clicked() {
                self.target_group.push(TargetSpecVec {
                    constant: 1.0,
                    coefficients: Vec::new(),
                });
            }
        });
    }
}

fn update_fluid_metainfo(
    fluid_temperaturess: &mut AIndexMap<String, AIndexSet<[i32; 2]>>,
    fluid_fuels: &mut AIndexSet<String>,
    fluid_heats: &mut AIndexSet<String>,
    item: &DualVar,
) {
    match item {
        DualVar::Fluid { name, temperature } => {
            fluid_temperaturess
                .entry(name.clone())
                .or_default()
                .insert(*temperature);
        }
        DualVar::FluidFuel {
            filter: Some(filter),
        } => {
            fluid_fuels.insert(filter.clone());
        }
        DualVar::FluidHeat {
            filter: Some(filter),
        } => {
            fluid_heats.insert(filter.clone());
        }
        _ => {}
    }
}

impl SolveContext for FactoryInstance {
    type Game = DataContext;
    type Item = DualVar;
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
                ui.heading(t!("metatorio.recipe-config").to_string());
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
                egui::containers::menu::MenuBar::new().ui(ui, |ui| {
                    self.mechanics.iter().enumerate().for_each(|(id, m)| {
                        ui.selectable_value(&mut self.suggesting_mechanic, id, m.name());
                    });
                });
                ui.separator();
                if let Some(mechanic) = self.mechanics.get_mut(self.suggesting_mechanic) {
                    ui.heading(mechanic.name());
                    changed |= mechanic.suggestion_view(ui, data, proj, &self.factory);
                }
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
    pub problem_sender: Sender<(usize, SolverData<DualVar, (usize, usize)>)>,
    #[serde(skip)]
    pub solution_receiver: Receiver<(usize, SolverSolution<DualVar, (usize, usize)>)>,
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

            name: t!("metatorio.unnamed-project").to_string(),
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

    pub fn set_default_milestones(&mut self) {
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
    }

    pub fn with_default_milestones(mut self) -> Self {
        self.set_default_milestones();
        self
    }

    pub fn reset_factory_channel(&mut self) {
        let (factory_tx, factory_rx) = channel();
        self.proj.factory_sender = Some(factory_tx);
        self.factory_receiver = factory_rx;
    }

    pub fn post_load(mut self) -> Self {
        let makeup_factory = FactoryInstance::default();
        for factory in &mut self.factories.vec {
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
        self.reset_factory_channel();
        self.proj
            .tech_milestones
            .retain(|(tech_name, _)| self.data.technologies.contains_key(tech_name));
        update_accessibles(&mut self.proj, &self.data);
        self.proj.milestone_graph = resolve_milestone_graph(&self.data, &self.proj.tech_milestones);
        self.proj
            .tech_milestones
            .sort_by_cached_key(|v| (self.proj.milestone_graph[v.0.as_str()].depth, v.0.clone()));
        self.request_solution();
        self
    }

    pub fn request_solution(&mut self) {
        self.factories
            .vec
            .iter_mut()
            .enumerate()
            .for_each(|(idx, f)| {
                let _ = self
                    .problem_sender
                    .send((idx, f.as_problem(&self.data, &self.proj)));
            });
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

                    sort_generic_items_owned(&mut factory.total_flow_sorted_keys, &self.data);
                    factory
                        .total_flow_sorted_keys
                        .sort_by(|ka, kb| sum[ka].signum().partial_cmp(&sum[kb].signum()).unwrap());

                    factory.solution = result;
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
                            if ui.button(t!("metatorio.preferences")).clicked() {
                                self.proj.selected_page = ProjectPage::UserContext;
                            }
                            if ui.button(t!("metatorio.new-factory")).clicked() {
                                let name = t!("metatorio.new-factory-name").to_string();
                                self.factories
                                    .push(FactoryInstance::new(name).with_default_mechanics());
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
                            ui.heading(t!("metatorio.preference").to_string());
                            ui.separator();
                            self.proj.saved &= !ui
                                .add(UserContextEditor::new(&self.data, &mut self.proj))
                                .changed();
                        });
                    }
                    ProjectPage::Index(page) => {
                        if self.factories.is_empty() {
                            let mut layout_job = egui::text::LayoutJob::default();
                            egui::RichText::new(t!("metatorio.no-factories").to_string())
                                .size(32.0)
                                .append_to(
                                    &mut layout_job,
                                    ui.style(),
                                    egui::FontSelection::Default,
                                    egui::Align::Center,
                                );
                            egui::RichText::new(t!("metatorio.create-factory-tooltip").to_string())
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
        t!(
            "metatorio.used-mods",
            self.data
                .mods
                .iter()
                .fold("".to_string(), |mut acc, (mod_name, mod_version)| {
                    acc.push_str(&format!("\n{} ({}), ", mod_name, mod_version));
                    acc
                },)
        )
        .to_string()
    }
}

pub struct ProjectView {
    pub data: Arc<DataContext>,
    pub selected: Option<usize>,
    pub projects: DndVec<ProjectInstance>,
    pub example_factory: Option<FactoryInstance>,
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
            example_factory: None,
            delete_request: DeleteRequest::None,
        }
    }
}

impl SubView for ProjectView {
    fn name(&self) -> String {
        t!("metatorio.factorio").to_string()
    }
    fn description(&self) -> String {
        t!(
            "metatorio.used-mods",
            self.data
                .mods
                .iter()
                .fold("".to_string(), |mut acc, (mod_name, mod_version)| {
                    acc.push_str(&format!("\n{} ({}), ", mod_name, mod_version));
                    acc
                },)
        )
        .to_string()
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
            ui.label(t!("metatorio.confirm-close").to_string());
            ui.horizontal(|ui| {
                if ui.button(t!("metatorio.cancel").to_string()).clicked() {
                    ui.close();
                }
                if ui
                    .button(t!("metatorio.close-program").to_string())
                    .clicked()
                {
                    self.ignore_close = true;
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui
                    .button(t!("metatorio.save-before-close").to_string())
                    .clicked()
                {
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
            ui.menu_button(t!("metatorio.file"), |ui| {
                if ui.button(t!("metatorio.new-project")).clicked() {
                    let mut project = ProjectInstance::new_arc(self.data.clone())
                        .with_default_milestones()
                        .post_load();
                    if let Some(example_factory) = self.example_factory.as_ref() {
                        let factory = example_factory.clone();
                        project.factories.push(factory);
                        project.proj.selected_page = ProjectPage::Index(0);
                    } else {
                        // 从 data 中随机选一个物品
                        let mut loop_count = 0;
                        for (item, prototype) in self.data.items.iter() {
                            if prototype.base.hidden || prototype.base.parameter {
                                continue;
                            }
                            let mut factory =
                                FactoryInstance::new(t!("metatorio.example-factory").to_string())
                                    .with_default_mechanics();
                            factory
                                .target
                                .push((DualVar::Item(item.clone().into()), 1.0));
                            let planet = self
                                .data
                                .planets
                                .keys()
                                .nth(loop_count % self.data.planets.len())
                                .cloned()
                                .unwrap();
                            factory.factory.planet = Some(planet.clone());

                            let auto_planned =
                                auto_planner_ref_silent(factory.clone(), &self.data, &project.proj);
                            if let Ok(auto_planned) = auto_planned {
                                self.example_factory = Some(auto_planned.clone());
                                project.factories.push(auto_planned);
                                break;
                            }
                            if loop_count > 8 {
                                break;
                            }
                            loop_count += 1;
                        }
                    }

                    project.request_solution();
                    project.proj.selected_page = ProjectPage::Index(0);
                    self.projects.push(project);
                    self.selected = Some(self.projects.len() - 1);
                    ui.close();
                }
                if ui.button(t!("metatorio.load-project")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter(t!("metatorio.project-file").to_string(), &["fpp"])
                        .set_title(t!("metatorio.open-project-file").to_string())
                        .pick_file()
                        && let Some(mut project) = load_project(&path)
                    {
                        project.set_data(self.data.clone());
                        project = project.post_load();
                        project.proj.saved = true;
                        project.proj.file_path = Some(path);
                        self.projects.push(project);
                        self.selected = Some(self.projects.len() - 1);
                    }
                    ui.close();
                }
                ui.add_enabled_ui(self.selected.is_some(), |ui| {
                    ui.separator();
                    if ui.button(t!("metatorio.save-project")).clicked() {
                        let project = &mut self.projects[self.selected.unwrap()];
                        if let Some(path) = &project.proj.file_path.clone() {
                            save_project(project, path);
                        } else {
                            save_project_as(project);
                        }
                        ui.close();
                    }
                    if ui.button(t!("metatorio.save-as")).clicked() {
                        let project = &mut self.projects[self.selected.unwrap()];
                        save_project_as(project);
                        ui.close();
                    }
                    if ui
                        .button(t!("metatorio.test-serialization-performance"))
                        .clicked()
                    {
                        let project = &mut self.projects[self.selected.unwrap()];
                        let instant = Instant::now();
                        let serialized = serde_json::to_string(project).unwrap();
                        log::info!(
                            "序列化耗时: {}ms, 大小: {}B",
                            instant.elapsed().as_millis(),
                            serialized.len()
                        );
                        let _: ProjectContext = serde_json::from_str(&serialized).unwrap();
                        log::info!("反序列化耗时: {}ms", instant.elapsed().as_millis());
                    }
                });
            })
        });
        ui.separator();
        let mut toggle = false;

        egui::containers::menu::MenuBar::new().ui(ui, |ui| {
            ui.label(t!("metatorio.project-list").to_string());

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
                    ui.label(t!("metatorio.confirm-delete").to_string());
                    ui.horizontal(|ui| {
                        if ui.button(t!("metatorio.cancel").to_string()).clicked() {
                            self.delete_request = DeleteRequest::None;
                            ui.close();
                        }
                        if ui
                            .button(t!("metatorio.delete-project").to_string())
                            .clicked()
                        {
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
        .add_filter(t!("metatorio.project-file").to_string(), &["fpp"])
        .set_title(t!("metatorio.save-project").to_string())
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
                crate::toast::info(t!("metatorio.project-saved"));
            }
            Err(e) => {
                crate::toast::error(t!("metatorio.save-failed", e.to_string()));
            }
        },
        Err(e) => {
            crate::toast::error(t!("metatorio.create-failed", e.to_string()));
        }
    }
}

pub fn load_project(path: &Path) -> Option<ProjectInstance> {
    match std::fs::File::open(path) {
        Ok(file) => match serde_json::from_reader(BufReader::new(file)) {
            Ok(proj) => Some(proj),
            Err(e) => {
                crate::toast::error(t!("metatorio.load-failed", e.to_string()));
                None
            }
        },
        Err(e) => {
            crate::toast::error(t!("metatorio.open-failed", e.to_string()));
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
    fn name(&self) -> String {
        t!("metatorio.factorio-planner").to_string()
    }
    fn view(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.heading(t!("metatorio.create-context").to_string());
            ui.separator();

            ui.label(t!("metatorio.select-game-path"));
            if ui.button(t!("metatorio.browse")).clicked()
                && let Some(path) = rfd::FileDialog::new().pick_file()
            {
                self.path = Some(path);
            }
            if let Some(path) = &self.path {
                ui.label(t!("metatorio.selected-path", path.display().to_string()));
                if path.to_string_lossy().contains("steam") {
                    ui.label(t!("metatorio.steam-version-warning"));
                }
            } else {
                ui.label(t!("metatorio.no-path-selected"));
            }

            ui.separator();

            ui.label(t!("metatorio.select-mod-path"));
            if ui.button(t!("metatorio.browse")).clicked() {
                if let Some(mod_path) = rfd::FileDialog::new().pick_folder() {
                    self.mod_path = Some(mod_path);
                } else {
                    self.mod_path = None;
                }
            }

            if let Some(mod_path) = &self.mod_path {
                ui.label(t!(
                    "metatorio.selected-mod-path",
                    mod_path.display().to_string()
                ));
            } else {
                ui.label(t!("metatorio.no-mod-path-selected"));
            }
            ui.separator();
            let mut can_load_context = true;
            if self.path.is_none() {
                ui.label(t!("metatorio.no-game-path-selected"));
                can_load_context = false;
            }
            if let Some(mod_path) = self.mod_path.as_ref()
                && !mod_path.join("mod-list.json").exists()
            {
                ui.label(t!("metatorio.mod-list-not-found"));
                can_load_context = false;
            }

            if self.thread.is_some() {
                ui.label(t!("metatorio.loading-context"));
                can_load_context = false;
            }

            ui.separator();

            if ui
                .add_enabled(
                    can_load_context,
                    egui::Button::new(t!("metatorio.load-game-context")),
                )
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
                            Some(&fust_i18n::get_locale()),
                        ) {
                            Ok(data) => {
                                sender
                                    .send(Box::new(ProjectView::new(data)))
                                    .expect("Failed to send subview");
                            }
                            Err(e) => {
                                crate::toast::error(t!(
                                    "metatorio.load-game-context-failed",
                                    format!("{:?}", e)
                                ));
                            }
                        },
                    ));
            }

            ui.separator();

            if ui
                .add_enabled(
                    self.thread.is_none(),
                    egui::Button::new(t!("metatorio.load-cache-context")),
                )
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
                                crate::toast::error(t!(
                                    "metatorio.load-cache-context-failed",
                                    format!("{:?}", e)
                                ));
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
