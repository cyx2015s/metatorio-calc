use crate::{
    concept::{Flow, SolveContext},
    factorio::{
        AsFlow, DataContext, DualVar, Effect, EnergyAmount, EnergySource, EntityPrototype,
        FactorioMechanic, FlowProxy, IdWithQuality, ProjectContext, ReactVec,
        SerdeFactorioMechanic, energy_source_as_flow, icon::Icon, index_map_update_entry,
        modal::SelectorModal, planner::FactoryContext, selector::Selector,
        surface_condition_satisfied,
    },
    math::flow_add,
};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ReactorPrototype {
    #[serde(flatten)]
    pub base: EntityPrototype,

    pub energy_source: EnergySource,

    pub consumption: EnergyAmount,
    #[serde(default)]
    pub neighbour_bonus: f64,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ReactorMechanic {
    #[serde(flatten)]
    pub instances: ReactVec<ReactorInstance>,
}

impl SolveContext for ReactorMechanic {
    type Game = DataContext;
    type Item = DualVar;
}

#[typetag::serde(name = "factorio:reactor")]
impl SerdeFactorioMechanic for ReactorMechanic {}

impl FactorioMechanic for ReactorMechanic {
    fn instances_proxy(&self) -> &dyn FlowProxy {
        &self.instances as &dyn FlowProxy
    }

    fn instances_proxy_mut(&mut self) -> &mut dyn FlowProxy {
        &mut self.instances as &mut dyn FlowProxy
    }

    fn editor_view(
        &mut self,
        ui: &mut egui::Ui,
        _data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        if ui.button("添加反应堆").clicked() {
            self.instances.push(ReactorInstance::default());
            changed = true;
        }
        changed
    }

    fn name(&self) -> String {
        "反应堆".into()
    }

    fn instance_view(
        &mut self,
        idx: usize,
        ui: &mut egui::Ui,
        data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        let instance = &mut self.instances[idx];
        ui.vertical(|ui| {
            ui.label("反应堆");
            let entity_button = ui.add_sized(
                [35.0, 35.0],
                Icon::new(data, "entity", &instance.reactor.0).with_quality(instance.reactor.1),
            );
            changed |= ui
                .add(
                    SelectorModal::new(entity_button.id, "选择反应堆")
                        .with_toggle(entity_button.clicked())
                        .with_selector(
                            Selector::new(data, "entity")
                                .with_current(&mut instance.reactor)
                                .chain_filter(|s: &IdWithQuality, f| f.reactors.contains_key(&s.0)),
                        ),
                )
                .changed();
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.label("邻居数量");
            changed |= ui
                .add(egui::DragValue::new(&mut instance.neighbours).range(0..=3))
                .changed();
        });
        changed
    }

    fn auto_populate(
        &mut self,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) {
        for reactor in data.reactors.values() {
            if let Some(surface_properties) = factory.get_current_surface_properties(data)
                && !surface_condition_satisfied(
                    &reactor.base.surface_conditions,
                    surface_properties,
                    &data.surface_properties,
                )
            {
                continue;
            }
            if proj.is_prototype_accessible("entity", &reactor.base.base.name) {
                // for quality in 0..=proj.max_quality_level {
                self.instances.push(ReactorInstance {
                    reactor: IdWithQuality(reactor.base.base.name.clone(), factory.major_quality),
                    neighbours: 3,
                    fuel: None,
                });
                // }
            }
        }
    }
}

#[test]
fn test_func() {
    let vec = [1, 2, 3, 4, 5, 6, 7, 8, 9];
    let ignore_4 = vec.into_iter().rev().skip(4).rev().collect::<Vec<_>>();
    dbg!(ignore_4);
}
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ReactorInstance {
    pub reactor: IdWithQuality,

    pub neighbours: u8,

    pub fuel: Option<(String, i32)>,
}

impl SolveContext for ReactorInstance {
    type Game = DataContext;
    type Item = DualVar;
}

impl AsFlow for ReactorInstance {
    fn as_flow(
        &self,
        data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> Flow<DualVar> {
        let mut flow = Flow::new();
        if let Some(reactor) = data.reactors.get(&self.reactor.0) {
            flow = flow_add(
                &flow,
                &energy_source_as_flow(
                    data,
                    &reactor.energy_source,
                    &reactor.consumption,
                    &Effect::default(),
                    &self.fuel,
                    &mut 1.0,
                ),
                1.0,
            );
            index_map_update_entry(
                &mut flow,
                DualVar::Heat,
                reactor.consumption.amount
                    * (1.0 + self.neighbours as f64 * reactor.neighbour_bonus)
                    * 60.0,
            );
        }
        let quality = (self.reactor.1 as usize).min(data.qualities.len() - 1);
        let multiplier = data.qualities[quality].default_multiplier();
        flow.iter_mut().for_each(|v| *v.1 *= multiplier);
        flow
    }

    fn cost(
        &self,
        _data: &DataContext,
        _proj: &crate::factorio::ProjectContext,
        _factory: &crate::factorio::planner::FactoryContext,
    ) -> f64 {
        if let Some(reactor) = _data.reactors.get(&self.reactor.0) {
            reactor
                .base
                .collision_box
                .as_ref()
                .map_or(100.0, |b| b.get_area())
        } else {
            100.0
        }
    }
}
