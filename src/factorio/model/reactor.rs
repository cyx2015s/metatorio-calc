use crate::{
    concept::{EntryOpRequest, EntryOpResult, Flow, SolveContext},
    factorio::{
        AsFlow, DataContext, Effect, EnergyAmount, EnergySource, EntityPrototype, FactorioMechanic,
        GenericItem, IdWithQuality, ProjectContext, energy_source_as_flow, icon::Icon,
        index_map_update_entry, modal::SelectorModal, planner::FactoryContext, selector::Selector,
    },
    math::{ElemVec, flow_add},
};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ReactorPrototype {
    #[serde(flatten)]
    pub base: EntityPrototype,

    pub energy_source: EnergySource,

    pub consumption: EnergyAmount,

    pub neighbour_bonus: f64,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ReactorMechanic {
    #[serde(skip)]
    pub operations: Vec<(usize, EntryOpRequest)>,

    pub instances: Vec<ReactorMechanicInstance>,
}

impl SolveContext for ReactorMechanic {
    type Game = DataContext;
    type Item = GenericItem;
}

#[typetag::serde(name = "factorio:reactor")]
impl FactorioMechanic for ReactorMechanic {
    fn editor_view(
        &mut self,
        ui: &mut egui::Ui,
        _data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        if ui.button("添加反应堆").clicked() {
            self.instances.push(ReactorMechanicInstance::default());
            changed = true;
        }
        changed
    }

    fn name(&self) -> String {
        "反应堆".into()
    }

    fn instances(&self) -> Vec<&dyn AsFlow> {
        self.instances.iter().map(|i| i as &dyn AsFlow).collect()
    }

    fn instance_len(&self) -> usize {
        self.instances.len()
    }

    fn instance_operate(
        &mut self,
        idx: usize,
        f: &mut dyn FnMut(&mut dyn AsFlow) -> EntryOpRequest,
    ) {
        let op = f(&mut self.instances[idx] as &mut dyn AsFlow);
        if !matches!(op, EntryOpRequest::None) {
            self.operations.push((idx, op));
        }
    }

    fn submit_operations(&mut self) -> Vec<EntryOpResult> {
        self.instances.update_elements(&mut self.operations)
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
            let entity_button = ui
                .add_sized(
                    [35.0, 35.0],
                    Icon::new(data, "entity", &instance.reactor.0).with_quality(instance.reactor.1),
                )
                .interact(egui::Sense::click());
            changed |= ui
                .add(
                    SelectorModal::new(entity_button.id, data, "选择反应堆")
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
        _factory: &FactoryContext,
    ) {
        for reactor in data.reactors.values() {
            for quality in 0..=proj.max_quality_level {
                self.instances.push(ReactorMechanicInstance {
                    reactor: IdWithQuality(reactor.base.base.name.clone(), quality),
                    neighbours: 3,
                    fuel: None,
                });
            }
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ReactorMechanicInstance {
    pub reactor: IdWithQuality,

    pub neighbours: u8,

    pub fuel: Option<(String, i32)>,
}

impl SolveContext for ReactorMechanicInstance {
    type Game = DataContext;
    type Item = GenericItem;
}

impl AsFlow for ReactorMechanicInstance {
    fn as_flow(
        &self,
        data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> Flow<GenericItem> {
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
                GenericItem::Heat,
                reactor.consumption.amount
                    * (1.0 + self.neighbours as f64 * reactor.neighbour_bonus)
                    * 60.0,
            );
        }
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
