use crate::{
    concept::{EntryOpRequest, Flow, SolveContext},
    factorio::{
        AsFlow, DataContext, Dict, EntityPrototype, FactorioMechanic, GenericItem, IdWithQuality,
        ProjectContext, RecipeResult, icon::Icon, index_map_update_entry, modal::SelectorModal,
        planner::FactoryContext, selector::Selector,
    },
    math::ElemVec,
};

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct PlantPrototype {
    #[serde(flatten)]
    pub base: EntityPrototype,

    pub growth_ticks: f64,
    #[serde(default)]
    pub harvest_emmisions: Dict<f64>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PlantMechanic {
    #[serde(skip)]
    pub operations: Vec<(usize, EntryOpRequest)>,

    pub instances: Vec<PlantInstance>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PlantInstance {
    pub seed: IdWithQuality,
}

impl SolveContext for PlantMechanic {
    type Game = DataContext;
    type Item = GenericItem;
}

impl SolveContext for PlantInstance {
    type Game = DataContext;
    type Item = GenericItem;
}

impl AsFlow for PlantInstance {
    fn as_flow(
        &self,
        data: &DataContext,
        _proj: &crate::factorio::ProjectContext,
        _factory: &crate::factorio::planner::FactoryContext,
    ) -> crate::concept::Flow<Self::Item> {
        let mut flow = Flow::new();
        if let Some(item) = data.items.get(&self.seed.0)
            && let Some(plant) = item.plant.as_ref()
        {
            let plant_result = &plant.plant_result;
            if let Some(plant) = data.plants.get(plant_result) {
                index_map_update_entry(
                    &mut flow,
                    GenericItem::Item(self.seed.clone()),
                    -1.0 / plant.growth_ticks * 60.0,
                );
                if let Some(minable) = plant.base.minable.as_ref() {
                    if let Some(result) = &minable.result {
                        index_map_update_entry(
                            &mut flow,
                            GenericItem::Item(result.clone().into()),
                            1.0 / plant.growth_ticks * 60.0,
                        );
                    } else {
                        for result in &minable.results {
                            if let RecipeResult::Item(item) = result {
                                index_map_update_entry(
                                    &mut flow,
                                    GenericItem::Item(item.name.clone().into()),
                                    item.normalized_output().0 / plant.growth_ticks * 60.0,
                                );
                            }
                        }
                    }
                }
            }
        }
        flow
    }

    fn cost(&self, _data: &DataContext, _proj: &ProjectContext, _factory: &FactoryContext) -> f64 {
        16.0
    }
}

#[typetag::serde(name = "factorio:plant")]
impl FactorioMechanic for PlantMechanic {
    fn name(&self) -> String {
        "种植".into()
    }

    fn editor_view(
        &mut self,
        ui: &mut egui::Ui,
        _data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        if ui.button("添加种树").clicked() {
            self.instances.push(PlantInstance::default());
            changed = true;
        }
        changed
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

    fn submit_operations(&mut self) -> Vec<crate::concept::EntryOpResult> {
        self.instances.update_elements(&mut self.operations)
    }

    fn auto_populate(
        &mut self,
        data: &DataContext,
        _proj: &ProjectContext,
        factory: &FactoryContext,
    ) {
        for item in data.items.values() {
            if let Some(plant_property) = item.plant.as_ref() {
                let _plant = data.plants.get(&plant_property.plant_result).unwrap();
                if let Some(planet) = factory.planet.as_ref()
                    && item
                        .default_import_location
                        .as_ref()
                        .is_some_and(|loc| loc == planet)
                {
                    self.instances.push(PlantInstance {
                        seed: item.base.name.clone().into(),
                    });
                }
            }
        }
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
            ui.label("种子");
            let button = ui
                .add_sized(
                    [35.0, 35.0],
                    Icon::new(data, "item", &instance.seed.0).with_quality(instance.seed.1),
                )
                .interact(egui::Sense::click());
            changed |= ui
                .add(
                    SelectorModal::new(button.id, data, "选择种子")
                        .with_toggle(button.clicked())
                        .with_selector(
                            Selector::new(data, "item")
                                .with_filter(|s: &IdWithQuality, f| {
                                    let item = f.items.get(&s.0);
                                    item.is_some_and(|i| i.plant.is_some())
                                })
                                .with_current(&mut instance.seed),
                        ),
                )
                .changed();
        });
        changed
    }
}
