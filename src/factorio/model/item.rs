use crate::factorio::common::*;

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

#[typetag::serde(name = "factorio:item-fuel")]
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
                    if let Some(item_prototype) = data.items.get(&instance.item.0)
                        && let Some(spoil) = &item_prototype.spoil
                        && let Some(spoil_result) = &spoil.spoil_result
                    {
                        ui.label(format!(
                            "变质时间: {}\n变质产物: {}",
                            spoil.spoil_ticks,
                            data.get_display_name("item", spoil_result)
                        ));
                    } else {
                        ui.label("无变质属性");
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
                                    if let Some(item_prototype) = data.items.get(&name.0)
                                        && let Some(spoil) = &item_prototype.spoil
                                        && let Some(spoil_result) = &spoil.spoil_result
                                    {
                                        ui.label(format!(
                                            "变质时间: {}\n变质产物: {}",
                                            spoil.spoil_ticks,
                                            data.get_display_name("item", spoil_result)
                                        ));
                                    } else {
                                        ui.label("无变质属性");
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
