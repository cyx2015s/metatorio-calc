use crate::concept::Flow;
use crate::factorio::hover::PrototypeHover;
use crate::factorio::{EntityPrototype, ItemResult, RecipeResult, common::*};

use crate::{
    concept::{EntryOpRequest, EntryOpResult, SolveContext},
    factorio::{
        AsFlow, DataContext, FactorioMechanic, GenericItem, IdWithQuality, ProjectContext,
        icon::Icon, modal::SelectorModal, planner::FactoryContext, selector::Selector,
    },
    math::ElemVec,
};

pub const ITEM_TYPES: &[&str] = &[
    "item",
    "ammo",
    "capsule",
    "gun",
    "item-with-entity-data",
    "item-with-label",
    "item-with-inventory",
    "blueprint-book",
    "item-with-tags",
    "selection-tool",
    "blueprint",
    "copy-paste-tool",
    "deconstruction-item",
    "spidertron-remote",
    "upgrade-item",
    "module",
    "rail-planner",
    "space-platform-starter-pack",
    "tool",
    "armor",
    "repair-tool",
];

/// 仅存储物品的基础属性，插件属性另行收集
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ItemPrototype {
    #[serde(flatten)]
    pub base: PrototypeBase,

    pub stack_size: f64,
    /// 变质可以自然发生，不绑定任何机器，所以属性存储在 Item 里
    #[serde(flatten)]
    pub spoil: Option<SpoilProperty>,

    /// 燃烧作为能量来源，可以发生在多种机器中，所以属性存储在 Item 里
    #[serde(flatten)]
    pub burn: Option<BurnProperty>,

    /// 种植实际上绑定农业塔，但完整的循环包括种子、植株、产物 3 个物品
    /// 另外所有物品都可以用作种子，没有单独的原型来区分，所以放这里最合适
    /// 农业塔不区分种子，种子也没有放置条件，是对应的植株有生长条件
    /// 所有考虑种植机制时，将植株本身存储为类配方，农业塔视作机器
    #[serde(flatten)]
    pub plant: Option<PlantProperty>,

    #[serde(default)]
    pub rocket_launch_products: Vec<ItemResult>,

    /// Tile
    pub place_as_tile: Option<PlaceAsTileProperty>,

    /// Entity
    pub place_result: Option<String>,

    /// 默认导入位置，辅助种植机制判断是否适合种植种子
    pub default_import_location: Option<String>,
}

impl HasPrototypeBase for ItemPrototype {
    fn base(&self) -> &PrototypeBase {
        &self.base
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SpoilProperty {
    pub spoil_ticks: f64,
    pub spoil_result: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct BurnProperty {
    pub fuel_value: EnergyAmount,
    pub burnt_result: Option<String>,
    pub fuel_category: Option<String>,
    pub fuel_emissions_multiplier: Option<f64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PlantProperty {
    pub plant_result: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PlaceAsTileProperty {
    pub result: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SpoilMechanic {
    #[serde(skip)]
    pub operations: Vec<(usize, EntryOpRequest)>,
    pub instances: Vec<SpoilInstance>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpoilInstance {
    pub item: IdWithQuality,
}

impl SolveContext for SpoilInstance {
    type Game = DataContext;
    type Item = GenericItem;
}

impl AsFlow for SpoilInstance {
    fn as_flow(
        &self,
        data: &super::DataContext,
        _proj: &crate::factorio::ProjectContext,
        _factory: &crate::factorio::planner::FactoryContext,
    ) -> crate::concept::Flow<Self::Item> {
        let mut flow = crate::concept::Flow::new();
        if let Some(item) = data.items.get(&self.item.0)
            && let Some(spoil) = &item.spoil
            && let Some(spoil_result) = &spoil.spoil_result
        {
            flow.insert(GenericItem::Item(self.item.clone()), -1.0);
            flow.insert(
                GenericItem::Item((spoil_result.clone(), self.item.1).into()),
                1.0,
            );
        }
        flow
    }

    fn cost(&self, data: &DataContext, _proj: &ProjectContext, _factory: &FactoryContext) -> f64 {
        if let Some(item) = data.items.get(&self.item.0)
            && let Some(spoil) = &item.spoil
        {
            spoil.spoil_ticks / item.stack_size / 16.0
        } else {
            0.0
        }
    }
}

impl SolveContext for SpoilMechanic {
    type Game = DataContext;
    type Item = GenericItem;
}

#[typetag::serde(name = "factorio:spoil")]
impl FactorioMechanic for SpoilMechanic {
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
                if i.spoil.is_some() && i.spoil.as_ref().unwrap().spoil_result.is_some() {
                    self.instances.push(SpoilInstance {
                        item: IdWithQuality(i.base.name.clone(), q),
                    });
                }
            }
        }
    }

    fn name(&self) -> String {
        "物品变质".to_string()
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

        if ui.button("添加物品变质").clicked() {
            let new_config = SpoilInstance {
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
            ui.label("变质物品");
            let item_button = ui
                .add_sized(
                    [35.0, 35.0],
                    Icon::new(data, "item", &instance.item.0).with_quality(instance.item.1),
                )
                .interact(egui::Sense::click())
                .on_hover_ui(|ui| {
                    if let Some(item_prototype) = data.items.get(&instance.item.0) {
                        ui.add(
                            PrototypeHover::new(data, item_prototype).with_quality(instance.item.1),
                        );
                    }
                });
            changed |= ui
                .add(
                    SelectorModal::new(item_button.id, data, "选择变质物品")
                        .with_toggle(item_button.clicked())
                        .with_selector(
                            Selector::new(data, "item")
                                .with_current(&mut instance.item)
                                .with_hover(|ui, name: &IdWithQuality, data| {
                                    if let Some(item_prototype) = data.items.get(&name.0) {
                                        ui.add(
                                            PrototypeHover::new(data, item_prototype)
                                                .with_quality(name.1),
                                        );
                                    }
                                })
                                .with_filter(|s, f| {
                                    f.items.get(&s.0).is_some_and(|i| {
                                        i.spoil.as_ref().is_some_and(|s| s.spoil_result.is_some())
                                    })
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

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct PlantPrototype {
    #[serde(flatten)]
    pub base: EntityPrototype,

    pub growth_ticks: f64,
    #[serde(default)]
    pub harvest_emissions: Dict<f64>,
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
                for harvest_emmision in &plant.harvest_emissions {
                    index_map_update_entry(
                        &mut flow,
                        GenericItem::Pollution {
                            name: harvest_emmision.0.clone(),
                        },
                        harvest_emmision.1 / plant.growth_ticks * 60.0,
                    );
                }
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
                if let Some(planet) = factory.planet.as_ref() {
                    match &item.default_import_location {
                        Some(loc) if loc == planet => self.instances.push(PlantInstance {
                            seed: item.base.name.clone().into(),
                        }),
                        None if planet == "nauvis" => self.instances.push(PlantInstance {
                            seed: item.base.name.clone().into(),
                        }),
                        _ => {}
                    }
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
            if let Some(burnt_result) = &item.burn.as_ref().and_then(|b| b.burnt_result.clone()) {
                flow.insert(GenericItem::Item(burnt_result.clone().into()), 1.0);
            }
        }
        flow
    }

    fn cost(&self, _data: &DataContext, _proj: &ProjectContext, _factory: &FactoryContext) -> f64 {
        // 1.0 / 1024.0 // 几乎无成本
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
                    if let Some(item_prototype) = data.items.get(&instance.item.0) {
                        ui.add(
                            PrototypeHover::new(data, item_prototype).with_quality(instance.item.1),
                        );
                    }
                });
            changed |= ui
                .add(
                    SelectorModal::new(item_button.id, data, "选择发射物")
                        .with_toggle(item_button.clicked())
                        .with_selector(
                            Selector::new(data, "item")
                                .with_current(&mut instance.item)
                                .with_hover(|ui, name: &IdWithQuality, data| {
                                    if let Some(item_prototype) = data.items.get(&name.0) {
                                        ui.add(
                                            PrototypeHover::new(data, item_prototype)
                                                .with_quality(name.1),
                                        );
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

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ItemLaunchMechanic {
    #[serde(skip)]
    pub operations: Vec<(usize, EntryOpRequest)>,
    pub instances: Vec<ItemLaunchInstance>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ItemLaunchInstance {
    pub item: IdWithQuality,
    pub rocket: (u16, bool), // 目前只支持按堆叠数限制的。
}

impl SolveContext for ItemLaunchInstance {
    type Game = DataContext;
    type Item = GenericItem;
}

impl AsFlow for ItemLaunchInstance {
    fn as_flow(
        &self,
        data: &super::DataContext,
        _proj: &crate::factorio::ProjectContext,
        _factory: &crate::factorio::planner::FactoryContext,
    ) -> crate::concept::Flow<Self::Item> {
        let mut flow = crate::concept::Flow::new();

        if let Some(item) = data.items.get(&self.item.0) {
            let multiplier = item.stack_size * self.rocket.0 as f64;
            index_map_update_entry(&mut flow, GenericItem::Item(self.item.clone()), -multiplier);
            index_map_update_entry(
                &mut flow,
                GenericItem::RocketCapacity {
                    stacks: self.rocket.0,
                    by_weight: self.rocket.1,
                },
                -1.0,
            );
            for result in &item.rocket_launch_products {
                let total_yield = result.normalized_output();
                index_map_update_entry(
                    &mut flow,
                    GenericItem::Item((result.name.clone(), self.item.1).into()),
                    (total_yield.0 + total_yield.1) * multiplier,
                );
            }
        }
        flow
    }

    fn cost(&self, _data: &DataContext, _proj: &ProjectContext, _factory: &FactoryContext) -> f64 {
        // 1.0 / 1024.0 // 几乎无成本
        0.0
    }
}

impl SolveContext for ItemLaunchMechanic {
    type Game = DataContext;
    type Item = GenericItem;
}

#[typetag::serde(name = "factorio:item-launch")]
impl FactorioMechanic for ItemLaunchMechanic {
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
                if !i.rocket_launch_products.is_empty() {
                    for rocket in data.rocket_types.values() {
                        self.instances.push(ItemLaunchInstance {
                            item: IdWithQuality(i.base.name.clone(), q),
                            rocket: *rocket,
                        });
                    }
                }
            }
        }
    }

    fn name(&self) -> String {
        "物品发射".to_string()
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
        data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        // 堆叠数大于 0，并且是按堆叠数限制的
        if data.rocket_types.iter().any(|(_, r)| r.0 > 0 && !r.1) {
            if ui.button("添加物品发射").clicked() {
                let new_config = ItemLaunchInstance {
                    item: IdWithQuality("".to_string(), 0),
                    rocket: *data
                        .rocket_types
                        .iter()
                        .find(|(_, r)| r.0 > 0 && !r.1)
                        .unwrap()
                        .1,
                };
                self.instances.push(new_config);
                changed = true;
            }
        } else {
            ui.label("无可用的火箭类型");
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
            ui.label("发射物品");
            let item_button = ui
                .add_sized(
                    [35.0, 35.0],
                    Icon::new(data, "item", &instance.item.0).with_quality(instance.item.1),
                )
                .interact(egui::Sense::click())
                .on_hover_ui(|ui| {
                    if let Some(item_prototype) = data.items.get(&instance.item.0) {
                        ui.add(
                            PrototypeHover::new(data, item_prototype).with_quality(instance.item.1),
                        );
                    }
                });
            changed |= ui
                .add(
                    SelectorModal::new(item_button.id, data, "选择发射物品")
                        .with_toggle(item_button.clicked())
                        .with_selector(
                            Selector::new(data, "item")
                                .with_current(&mut instance.item)
                                .with_hover(|ui, name: &IdWithQuality, data| {
                                    if let Some(item_prototype) = data.items.get(&name.0) {
                                        ui.add(
                                            PrototypeHover::new(data, item_prototype)
                                                .with_quality(name.1),
                                        );
                                    }
                                })
                                .with_filter(|s, f| {
                                    f.items
                                        .get(&s.0)
                                        .is_some_and(|i| !i.rocket_launch_products.is_empty())
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
