use indexmap::IndexMap;

use crate::{
    concept::{AsFlow, EditorView, Flow, Mechanic, MechanicProvider, MechanicSender, SolveContext},
    factorio::{
        IdWithQuality,
        editor::icon::GenericIcon,
        modal::{ItemSelectorModal, ItemWithQualitySelectorModal},
        model::context::{FactorioContext, GenericItem},
    },
};

/// 特殊：指代线性规划的无穷物体源
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename = "factorio:source")]
pub struct InfiniteSource {
    pub item: GenericItem,
}

impl SolveContext for InfiniteSource {
    type GameContext = FactorioContext;
    type ItemIdentType = GenericItem;
}

impl AsFlow for InfiniteSource {
    fn as_flow(&self, _ctx: &Self::GameContext) -> Flow<Self::ItemIdentType> {
        let mut map = IndexMap::new();
        map.insert(self.item.clone(), 1.0);
        map
    }

    fn cost(&self, _ctx: &Self::GameContext) -> f64 {
        // 返回一个默认较合理的成本
        match self.item {
            GenericItem::Electricity => 1.0 / 1024.0,
            _ => 1024.0,
        }
    }
}

impl EditorView for InfiniteSource {
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &Self::GameContext) {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                let label = ui.label("无限物体源");

                egui::ComboBox::from_id_salt(label.id)
                    .selected_text(match &self.item {
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
                            &mut self.item,
                            GenericItem::Item(IdWithQuality("item-unknown".to_string(), 0)),
                            "物品",
                        );
                        ui.selectable_value(
                            &mut self.item,
                            GenericItem::Fluid {
                                name: "fluid-unknown".to_string(),
                                temperature: None,
                            },
                            "流体",
                        );
                        ui.selectable_value(
                            &mut self.item,
                            GenericItem::Entity(IdWithQuality("entity-unknown".to_string(), 0)),
                            "实体",
                        );
                        ui.selectable_value(&mut self.item, GenericItem::Heat, "热量");
                        ui.selectable_value(&mut self.item, GenericItem::Electricity, "电力");
                        // 太复杂，先不开放这个功能，规划器也只尝试对物体、流体、实体本身进行配平，无视其他的需求
                        // ui.selectable_value(
                        //     &mut self.item,
                        //     GenericItem::FluidHeat { filter: None },
                        //     "流体热量",
                        // );
                        // ui.selectable_value(
                        //     &mut self.item,
                        //     GenericItem::FluidFuel { filter: None },
                        //     "流体燃料",
                        // );
                        // ui.selectable_value(
                        //     &mut self.item,
                        //     GenericItem::ItemFuel {
                        //         category: "fuel".to_string(),
                        //     },
                        //     "物体燃料",
                        // );
                        // ui.selectable_value(
                        //     &mut self.item,
                        //     GenericItem::RocketPayloadWeight,
                        //     "重量载荷",
                        // );
                        // ui.selectable_value(
                        //     &mut self.item,
                        //     GenericItem::RocketPayloadStack,
                        //     "堆叠载荷",
                        // );
                        // ui.selectable_value(
                        //     &mut self.item,
                        //     GenericItem::Pollution {
                        //         name: "pollution".to_string(),
                        //     },
                        //     "污染",
                        // );
                    });
            });
            let icon = ui
                .add_sized(
                    [35.0, 35.0],
                    GenericIcon {
                        ctx,
                        item: &self.item,
                        size: 32.0,
                    },
                )
                .interact(egui::Sense::click());
            match &mut self.item {
                GenericItem::Item(current) => {
                    ui.add(
                        ItemWithQualitySelectorModal::new(ctx, "选择去皮", "item", &icon)
                            .with_current(current),
                    );
                }
                GenericItem::Fluid { .. } => {
                    let mut sentinel = None;
                    ui.add(
                        ItemSelectorModal::new(ctx, "选择流体", "fluid", &icon)
                            .with_output(&mut sentinel),
                    );
                    if let Some(name) = sentinel {
                        self.item = GenericItem::Fluid {
                            name: name.clone(),
                            temperature: None,
                        };
                    }
                }
                GenericItem::Entity(current) => {
                    ui.add(
                        ItemWithQualitySelectorModal::new(ctx, "选择实体", "entity", &icon)
                            .with_current(current),
                    );
                }
                _ => {}
            }
        });
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InfiniteSourceProvider {
    #[serde(skip)]
    pub sender: Option<MechanicSender<GenericItem, FactorioContext>>,
}

impl Default for InfiniteSourceProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl InfiniteSourceProvider {
    pub fn new() -> Self {
        Self { sender: None }
    }
}

impl SolveContext for InfiniteSourceProvider {
    type GameContext = FactorioContext;
    type ItemIdentType = GenericItem;
}

impl EditorView for InfiniteSourceProvider {
    fn editor_view(&mut self, ui: &mut egui::Ui, _ctx: &Self::GameContext) {
        if ui.button("添加无限源").clicked() {
            let source = InfiniteSource {
                item: GenericItem::Item(IdWithQuality("item-unknown".to_string(), 0)),
            };
            if let Some(sender) = &self.sender {
                let _ = sender.send(Box::new(source));
            }
        }
    }
}

impl MechanicProvider for InfiniteSourceProvider {
    fn set_mechanic_sender(&mut self, sender: MechanicSender<GenericItem, FactorioContext>) {
        self.sender = Some(sender);
    }

    fn hint_populate(
        &self,
        _ctx: &Self::GameContext,
        item: &Self::ItemIdentType,
        value: f64,
    ) -> Vec<
        Box<
            dyn crate::concept::Mechanic<
                    ItemIdentType = Self::ItemIdentType,
                    GameContext = Self::GameContext,
                >,
        >,
    > {
        if value < 0.0 {
            let source = InfiniteSource { item: item.clone() };
            vec![Box::new(source)]
        } else {
            vec![]
        }
    }
}

crate::impl_register_deserializer!(
    for InfiniteSource
    as "factorio:source"
    => dyn Mechanic<ItemIdentType = GenericItem, GameContext = FactorioContext>
);
