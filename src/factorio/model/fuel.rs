use crate::{
    concept::{EntryOpRequest, EntryOpResult, SolveContext},
    factorio::{
        AsFlow, DataContext, FactorioMechanic, GenericItem, IdWithQuality, ProjectContext,
        icon::Icon, modal::SelectorModal, planner::FactoryContext, selector::Selector,
    },
    math::ElemVec,
};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ItemFuelMechanic {
    #[serde(skip)]
    pub operations: Vec<(usize, EntryOpRequest)>,
    pub instances: Vec<ItemFuelInstance>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ItemFuelInstance {
    pub item: IdWithQuality,
}

impl SolveContext for ItemFuelInstance {
    type Game = DataContext;
    type Item = GenericItem;
}

impl AsFlow for ItemFuelInstance {
    fn as_flow(
        &self,
        data: &super::DataContext,
        _proj: &crate::factorio::ProjectContext,
        _factory: &crate::factorio::planner::FactoryContext,
    ) -> crate::concept::Flow<Self::Item> {
        let mut flow = crate::concept::Flow::new();
        if let Some(item) = data.items.get(&self.item.0) {
            flow.insert(GenericItem::Item(self.item.clone()), -1.0);
            flow.insert(
                GenericItem::ItemFuel {
                    category: item.burn.as_ref().map_or("chemical".to_string(), |b| {
                        b.fuel_category.clone().unwrap_or("chemical".to_string())
                    }),
                },
                item.burn.as_ref().map_or(0.0, |b| b.fuel_value.amount),
            );
        }
        flow
    }

    fn cost(&self, _data: &DataContext, _proj: &ProjectContext, _factory: &FactoryContext) -> f64 {
        0.0
    }
}

impl SolveContext for ItemFuelMechanic {
    type Game = DataContext;
    type Item = GenericItem;
}

#[typetag::serde(name = "factorio:item-fuel")]
impl FactorioMechanic for ItemFuelMechanic {
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

    fn update_suggestion(
        &mut self,
        _data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
        _item: &GenericItem,
        _amount: f64,
    ) {
    }

    #[allow(unused_variables)]
    fn suggestion_view(
        &mut self,
        ui: &mut egui::Ui,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) -> bool {
        false
    }

    #[allow(unused_variables)]
    fn auto_populate(
        &mut self,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) {
        for q in 0..=proj.max_quality_level {
            for i in data.items.values() {
                if i.burn.is_some() {
                    self.instances.push(ItemFuelInstance {
                        item: IdWithQuality(i.base.name.clone(), q),
                    });
                }
            }
        }
    }

    fn name(&self) -> String {
        "燃烧物品".to_string()
    }

    fn instances(&self) -> Vec<&dyn AsFlow> {
        self.instances
            .iter()
            .map(|instance| instance as &dyn AsFlow)
            .collect()
    }

    fn instance_len(&self) -> usize {
        self.instances.len()
    }

    fn editor_view(
        &mut self,
        ui: &mut egui::Ui,
        _data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;

        if ui.button("添加物品燃烧").clicked() {
            let new_config = ItemFuelInstance {
                item: IdWithQuality("".to_string(), 0),
            };
            self.instances.push(new_config);
            changed = true;
        }
        changed
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
            ui.label("燃料");
            let item_button = ui
                .add_sized(
                    [35.0, 35.0],
                    Icon::new(data, "item", &instance.item.0).with_quality(instance.item.1),
                )
                .interact(egui::Sense::click())
                .on_hover_ui(|ui| {
                    if let Some(item_prototype) = data.items.get(&instance.item.0)
                        && let Some(burn) = &item_prototype.burn
                    {
                        ui.label(format!(
                            "燃料热值: {}\n燃料类别: {}",
                            burn.fuel_value,
                            burn.fuel_category.clone().unwrap_or("chemical".to_string())
                        ));
                    }
                });
            changed |= ui
                .add(
                    SelectorModal::new(item_button.id, data, "选择燃料")
                        .with_toggle(item_button.clicked())
                        .with_selector(
                            Selector::new(data, "item")
                                .with_current(&mut instance.item)
                                .with_hover(|ui, name: &IdWithQuality, data| {
                                    if let Some(item_prototype) = data.items.get(&name.0)
                                        && let Some(burn) = &item_prototype.burn
                                    {
                                        ui.label(format!(
                                            "燃料热值: {}\n燃料类别: {}",
                                            burn.fuel_value,
                                            burn.fuel_category
                                                .clone()
                                                .unwrap_or("chemical".to_string())
                                        ));
                                    }
                                })
                                .with_filter(|s, f| {
                                    f.items.get(&s.0).is_some_and(|i| i.burn.is_some())
                                }),
                        ),
                )
                .changed();
        });
        changed
    }

    fn submit_operations(&mut self) -> Vec<EntryOpResult> {
        self.instances.update_elements(&mut self.operations)
    }
}
