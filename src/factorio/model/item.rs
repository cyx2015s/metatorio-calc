use crate::concept::Flow;
use crate::factorio::{EntityPrototype, ItemResult, RecipeResult, common::*};

use crate::{
    concept::SolveContext,
    factorio::{
        AsFlow, DataContext, DualVar, FactorioMechanic, IdWithQuality, ProjectContext, icon::Icon,
        modal::SelectorModal, planner::FactoryContext, selector::Selector,
    },
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

    /// 物品重量，用于计算火箭运力。如果未设置，使用 default_item_weight (100)
    pub weight: Option<f64>,
    /// 物品的 ingredient_to_weight_coefficient，如果未设置，默认为 0.5
    pub ingredient_to_weight_coefficient: Option<f64>,
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
    #[serde(flatten)]
    pub instances: ReactVec<SpoilInstance>,
    #[serde(skip)]
    pub suggestion_item: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SpoilInstance {
    pub item: IdWithQuality,
}

impl SolveContext for SpoilInstance {
    type Game = DataContext;
    type Item = DualVar;
}

impl AsFlow for SpoilInstance {
    fn as_flow(
        &self,
        data: &super::DataContext,
        _proj: &crate::factorio::ProjectContext,
        _factory: &crate::factorio::planner::FactoryContext,
    ) -> crate::concept::Flow<Self::Item> {
        let mut flow = crate::concept::Flow::default();
        if let Some(item) = data.items.get(&self.item.0)
            && let Some(spoil) = &item.spoil
            && let Some(spoil_result) = &spoil.spoil_result
        {
            flow.insert(DualVar::Item(self.item.clone()), -1.0);
            flow.insert(
                DualVar::Item((spoil_result.clone(), self.item.1).into()),
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
    type Item = DualVar;
}

#[typetag::serde(name = "factorio:spoil")]
impl SerdeFactorioMechanic for SpoilMechanic {}

impl FactorioMechanic for SpoilMechanic {
    fn instances_proxy(&self) -> &dyn FlowProxy {
        &self.instances as &dyn FlowProxy
    }

    fn instances_proxy_mut(&mut self) -> &mut dyn FlowProxy {
        &mut self.instances as &mut dyn FlowProxy
    }

    fn update_suggestion(
        &mut self,
        data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
        item: &DualVar,
        amount: f64,
    ) {
        if let DualVar::Item(item) = item {
            if amount < 0.0 {
                self.suggestion_item = Some(item.0.clone());
            } else if amount > 0.0 {
                for item in data.items.values() {
                    if item
                        .spoil
                        .as_ref()
                        .is_some_and(|s| s.spoil_result.as_ref() == Some(&item.base.name))
                    {
                        self.suggestion_item = Some(item.base.name.clone());
                        break;
                    }
                }
            }
        }
    }

    #[allow(unused_variables)]
    fn suggestion_view(
        &mut self,
        ui: &mut egui::Ui,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) -> bool {
        let mut new_item = None;
        ui.add(
            Selector::new(data, "item")
                .with_output(&mut new_item)
                .with_filter(|s: &IdWithQuality, f| {
                    self.suggestion_item.as_ref().is_some_and(|t| t == &s.0)
                }),
        );
        if let Some(new_item) = new_item {
            self.instances.push(SpoilInstance { item: new_item });
            true
        } else {
            false
        }
    }

    #[allow(unused_variables)]
    fn auto_populate(
        &mut self,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) {
        for q in 0..=proj.max_quality() {
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
        t!("metatorio.spoil").to_string()
    }

    fn editor_view(
        &mut self,
        ui: &mut egui::Ui,
        _data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;

        if ui
            .button(t!("metatorio.add-spoil").to_string().as_str())
            .clicked()
        {
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
            ui.label(t!("metatorio.spoil-item"));
            let item_button = ui.add_sized(
                [35.0, 35.0],
                Icon::new(data, "item", &instance.item.0).with_quality(instance.item.1),
            );

            changed |= ui
                .add(
                    SelectorModal::new(
                        item_button.id,
                        t!("metatorio.select-item").to_string().as_str(),
                    )
                    .with_toggle(item_button.clicked())
                    .with_selector(
                        Selector::new(data, "item")
                            .with_current(&mut instance.item)
                            .with_filter(|s: &IdWithQuality, f| {
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
    #[serde(flatten)]
    pub instances: ReactVec<PlantInstance>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PlantInstance {
    pub seed: IdWithQuality,
}

impl SolveContext for PlantMechanic {
    type Game = DataContext;
    type Item = DualVar;
}

impl SolveContext for PlantInstance {
    type Game = DataContext;
    type Item = DualVar;
}

impl AsFlow for PlantInstance {
    fn as_flow(
        &self,
        data: &DataContext,
        _proj: &crate::factorio::ProjectContext,
        _factory: &crate::factorio::planner::FactoryContext,
    ) -> crate::concept::Flow<Self::Item> {
        let mut flow = Flow::default();
        if let Some(item) = data.items.get(&self.seed.0)
            && let Some(plant) = item.plant.as_ref()
        {
            let plant_result = &plant.plant_result;
            if let Some(plant) = data.plants.get(plant_result) {
                index_map_update_entry(
                    &mut flow,
                    DualVar::Item(self.seed.clone()),
                    -1.0 / plant.growth_ticks * 60.0,
                );
                for harvest_emmision in &plant.harvest_emissions {
                    index_map_update_entry(
                        &mut flow,
                        DualVar::Pollution {
                            name: harvest_emmision.0.clone(),
                        },
                        harvest_emmision.1 / plant.growth_ticks * 60.0,
                    );
                }
                if let Some(minable) = plant.base.minable.as_ref() {
                    if let Some(result) = &minable.result {
                        index_map_update_entry(
                            &mut flow,
                            DualVar::Item(result.clone().into()),
                            1.0 / plant.growth_ticks * 60.0,
                        );
                    } else {
                        for result in &minable.results {
                            if let RecipeResult::Item(item) = result {
                                index_map_update_entry(
                                    &mut flow,
                                    DualVar::Item(item.name.clone().into()),
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
impl SerdeFactorioMechanic for PlantMechanic {}
impl FactorioMechanic for PlantMechanic {
    fn name(&self) -> String {
        t!("metatorio.plant").to_string()
    }

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
        if ui.button(t!("metatorio.add-plant")).clicked() {
            self.instances.push(PlantInstance::default());
            changed = true;
        }
        changed
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
                    for quality in 0..data.qualities.len() {
                        match &item.default_import_location {
                            Some(loc) if loc == planet => self.instances.push(PlantInstance {
                                seed: (item.base.name.clone(), quality as u8).into(),
                            }),
                            None if planet == "nauvis" => self.instances.push(PlantInstance {
                                seed: (item.base.name.clone(), quality as u8).into(),
                            }),
                            _ => {}
                        }
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
            ui.label(t!("metatorio.plant-seed"));
            let button = ui.add_sized(
                [35.0, 35.0],
                Icon::new(data, "item", &instance.seed.0).with_quality(instance.seed.1),
            );
            changed |= ui
                .add(
                    SelectorModal::new(button.id, t!("metatorio.select-item").to_string().as_str())
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
    #[serde(flatten)]
    pub instances: ReactVec<ItemFuelInstance>,

    #[serde(skip)]
    pub suggested_category: Option<String>,
    #[serde(skip)]
    pub suggested_item: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ItemFuelInstance {
    pub item: IdWithQuality,
}

impl SolveContext for ItemFuelInstance {
    type Game = DataContext;
    type Item = DualVar;
}

impl AsFlow for ItemFuelInstance {
    fn as_flow(
        &self,
        data: &super::DataContext,
        _proj: &crate::factorio::ProjectContext,
        _factory: &crate::factorio::planner::FactoryContext,
    ) -> crate::concept::Flow<Self::Item> {
        let mut flow = crate::concept::Flow::default();
        if let Some(item) = data.items.get(&self.item.0) {
            flow.insert(DualVar::Item(self.item.clone()), -1.0);
            flow.insert(
                DualVar::ItemFuel {
                    category: item.burn.as_ref().map_or("chemical".to_string(), |b| {
                        b.fuel_category.clone().unwrap_or("chemical".to_string())
                    }),
                },
                item.burn.as_ref().map_or(0.0, |b| b.fuel_value.amount),
            );
            if let Some(burnt_result) = &item.burn.as_ref().and_then(|b| b.burnt_result.clone()) {
                flow.insert(DualVar::Item(burnt_result.clone().into()), 1.0);
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
    type Item = DualVar;
}

#[typetag::serde(name = "factorio:item-fuel")]
impl SerdeFactorioMechanic for ItemFuelMechanic {}

impl FactorioMechanic for ItemFuelMechanic {
    fn instances_proxy(&self) -> &dyn FlowProxy {
        &self.instances as &dyn FlowProxy
    }

    fn instances_proxy_mut(&mut self) -> &mut dyn FlowProxy {
        &mut self.instances as &mut dyn FlowProxy
    }

    fn update_suggestion(
        &mut self,
        _data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
        item: &DualVar,
        _amount: f64,
    ) {
        match item {
            DualVar::ItemFuel { category } => {
                if _amount > 0.0 {
                    self.suggested_category = Some(category.clone());
                    self.suggested_item = None;
                }
            }
            DualVar::Item(item) => {
                if _amount < 0.0 {
                    self.suggested_item = Some(item.0.clone());
                    self.suggested_category = None;
                }
            }
            _ => {}
        }
    }

    #[allow(unused_variables)]
    fn suggestion_view(
        &mut self,
        ui: &mut egui::Ui,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) -> bool {
        let mut new_fuel = None;
        ui.add(
            Selector::new(data, "item")
                .with_output(&mut new_fuel)
                .with_filter(|s: &IdWithQuality, f| {
                    if let Some(item) = f.items.get(&s.0) {
                        item.burn.as_ref().is_some_and(|b| {
                            match (self.suggested_category.as_ref(), b.fuel_category.as_ref()) {
                                (Some(suggested), Some(fuel_cat)) => suggested == fuel_cat,
                                (Some(suggested), None) => suggested == "chemical", // 默认类别为 chemical
                                (None, _) => {
                                    self.suggested_item.as_ref().is_some_and(|t| t == &s.0)
                                }
                            }
                        })
                    } else {
                        false
                    }
                }),
        );
        if let Some(new_fuel) = new_fuel {
            self.instances.push(ItemFuelInstance { item: new_fuel });
            true
        } else {
            false
        }
    }

    #[allow(unused_variables)]
    fn auto_populate(
        &mut self,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) {
        for q in 0..=proj.cur_max_quality_level {
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
        t!("metatorio.fuel-item").to_string()
    }

    fn editor_view(
        &mut self,
        ui: &mut egui::Ui,
        _data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;

        if ui.button(t!("metatorio.add-fuel-item")).clicked() {
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
            ui.label(t!("metatorio.fuel-item"));
            let item_button = ui.add_sized(
                [35.0, 35.0],
                Icon::new(data, "item", &instance.item.0).with_quality(instance.item.1),
            );
            changed |= ui
                .add(
                    SelectorModal::new(
                        item_button.id,
                        t!("metatorio.select-item").to_string().as_str(),
                    )
                    .with_toggle(item_button.clicked())
                    .with_selector(
                        Selector::new(data, "item")
                            .with_current(&mut instance.item)
                            .with_filter(|s: &IdWithQuality, f| {
                                f.items.get(&s.0).is_some_and(|i| i.burn.is_some())
                            }),
                    ),
                )
                .changed();
        });
        changed
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ItemLaunchMechanic {
    #[serde(flatten)]
    pub instances: ReactVec<ItemLaunchInstance>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ItemLaunchInstance {
    pub item: IdWithQuality,
    pub rocket: (u16, bool), // 目前只支持按堆叠数限制的。
}

impl SolveContext for ItemLaunchInstance {
    type Game = DataContext;
    type Item = DualVar;
}

impl AsFlow for ItemLaunchInstance {
    fn as_flow(
        &self,
        data: &super::DataContext,
        _proj: &crate::factorio::ProjectContext,
        _factory: &crate::factorio::planner::FactoryContext,
    ) -> crate::concept::Flow<Self::Item> {
        let mut flow = crate::concept::Flow::default();

        if let Some(item) = data.items.get(&self.item.0) {
            let (multiplier, capacity_var, capacity_cost) = if self.rocket.1 {
                // 重量火箭：装载量 = 火箭抬升重量 / 物品重量
                let item_weight = item.weight.unwrap_or(data.default_item_weight);
                let lift = data.rocket_lift_weight;
                (lift / item_weight, DualVar::RocketWeightCapacity, lift)
            } else {
                // 堆叠火箭：装载量 = 槽数 * 每槽堆叠数
                let stacks = self.rocket.0;
                (
                    item.stack_size * stacks as f64,
                    DualVar::RocketSlotCapacity,
                    stacks as f64,
                )
            };
            index_map_update_entry(&mut flow, DualVar::Item(self.item.clone()), -multiplier);
            index_map_update_entry(
                &mut flow,
                capacity_var,
                -capacity_cost,
            );
            for result in &item.rocket_launch_products {
                let total_yield = result.normalized_output();
                index_map_update_entry(
                    &mut flow,
                    DualVar::Item((result.name.clone(), self.item.1).into()),
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
    type Item = DualVar;
}

#[typetag::serde(name = "factorio:item-launch")]
impl SerdeFactorioMechanic for ItemLaunchMechanic {}
impl FactorioMechanic for ItemLaunchMechanic {
    fn instances_proxy(&self) -> &dyn FlowProxy {
        &self.instances as &dyn FlowProxy
    }

    fn instances_proxy_mut(&mut self) -> &mut dyn FlowProxy {
        &mut self.instances as &mut dyn FlowProxy
    }

    fn update_suggestion(
        &mut self,
        _data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
        _item: &DualVar,
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
        let rocket_types: Vec<(u16, bool)> = data.crafters.values()
            .filter(|c| &c.base.base.r#type == "rocket-silo")
            .map(|c| {
                if c.launch_to_space_platforms {
                    (0u16, true)
                } else {
                    (c.to_be_inserted_to_rocket_inventory_size as u16, false)
                }
            })
            .collect();
        for q in 0..=proj.cur_max_quality_level {
            for i in data.items.values() {
                if !i.rocket_launch_products.is_empty() {
                    for rocket in &rocket_types {
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
        t!("metatorio.item-launch").to_string()
    }

    fn editor_view(
        &mut self,
        ui: &mut egui::Ui,
        data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        let rocket_types: Vec<(u16, bool)> = data.crafters.values()
            .filter(|c| &c.base.base.r#type == "rocket-silo")
            .map(|c| {
                if c.launch_to_space_platforms {
                    (0u16, true)
                } else {
                    (c.to_be_inserted_to_rocket_inventory_size as u16, false)
                }
            })
            .collect();
        
        if !rocket_types.is_empty() {
            if ui.button(t!("metatorio.add-item-launch")).clicked() {
                let new_config = ItemLaunchInstance {
                    item: IdWithQuality("".to_string(), 0),
                    rocket: rocket_types[0],
                };
                self.instances.push(new_config);
                changed = true;
            }
        } else {
            ui.label(t!("metatorio.no-available-rocket-types"));
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
            ui.label(t!("metatorio.launch-item"));
            let item_button = ui.add_sized(
                [35.0, 35.0],
                Icon::new(data, "item", &instance.item.0).with_quality(instance.item.1),
            );
            changed |= ui
                .add(
                    SelectorModal::new(
                        item_button.id,
                        t!("metatorio.select-item").to_string().as_str(),
                    )
                    .with_toggle(item_button.clicked())
                    .with_selector(
                        Selector::new(data, "item")
                            .with_current(&mut instance.item)
                            .with_filter(|s: &IdWithQuality, f| {
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
}