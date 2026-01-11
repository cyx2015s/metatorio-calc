use crate::{
    concept::{AsFlow, AsFlowEditor, AsFlowEditorSource, AsFlowSender, ContextBound, EditorView},
    factorio::{
        editor::{
            icon::GenericIcon,
            selector::{ItemSelector, selector_menu_with_filter},
        },
        model::{
            context::{FactorioContext, GenericItem},
            item,
        },
    },
};

/// 特殊：指代线性规划的无穷物体源
pub struct SourceConfig {
    pub item: GenericItem,
}

impl ContextBound for SourceConfig {
    type ContextType = FactorioContext;
    type ItemIdentType = GenericItem;
}

impl AsFlow for SourceConfig {
    fn as_flow(
        &self,
        ctx: &Self::ContextType,
    ) -> std::collections::HashMap<Self::ItemIdentType, f64> {
        let mut map = std::collections::HashMap::new();
        map.insert(self.item.clone(), 1.0);
        map
    }

    fn cost(&self, _ctx: &Self::ContextType) -> f64 {
        // 返回一个较大的成本
        1024.0
    }
}

impl EditorView for SourceConfig {
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &Self::ContextType) {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                let label = ui.label("无限物体源");

                egui::ComboBox::from_id_salt(label.id)
                    .selected_text(match &self.item {
                        GenericItem::Item { .. } => "物体",
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
                            GenericItem::Item {
                                name: "item-unknown".to_string(),
                                quality: 0,
                            },
                            "物体",
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
                            GenericItem::Entity {
                                name: "entity-unknown".to_string(),
                                quality: 0,
                            },
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
                        ctx: &ctx,
                        item: &self.item,
                        size: 32.0,
                    },
                )
                .interact(egui::Sense::click());
            match &self.item {
                GenericItem::Item { name, quality } => {
                    if let Some(selected) = selector_menu_with_filter(
                        ui,
                        ctx,
                        "选择物体",
                        "item",
                        ctx.item_order.as_ref().unwrap(),
                        icon,
                    ) {
                        self.item = GenericItem::Item {
                            name: selected,
                            quality: *quality,
                        };
                    }
                }
                GenericItem::Fluid {
                    name,
                    temperature: _,
                } => {
                    if let Some(selected) = selector_menu_with_filter(
                        ui,
                        ctx,
                        "选择流体",
                        "fluid",
                        ctx.fluid_order.as_ref().unwrap(),
                        icon,
                    ) {
                        self.item = GenericItem::Fluid {
                            name: selected,
                            temperature: None,
                        };
                    }
                }
                GenericItem::Entity { name, quality } => {
                    if let Some(selected) = selector_menu_with_filter(
                        ui,
                        ctx,
                        "选择实体",
                        "entity",
                        ctx.entity_order.as_ref().unwrap(),
                        icon,
                    ) {
                        self.item = GenericItem::Entity {
                            name: selected,
                            quality: *quality,
                        };
                    }
                }
                _ => {}
            }
        });
    }
}

impl AsFlowEditor for SourceConfig {}

pub struct SourceConfigSource {
    pub sender: AsFlowSender<GenericItem, FactorioContext>,
}

impl ContextBound for SourceConfigSource {
    type ContextType = FactorioContext;
    type ItemIdentType = GenericItem;
}

impl EditorView for SourceConfigSource {
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &Self::ContextType) {
        if ui.button("添加无限源").clicked() {
            let source = SourceConfig {
                item: GenericItem::Item {
                    name: "item-unknown".to_string(),
                    quality: 0,
                },
            };
            self.sender.send(Box::new(source)).unwrap();
        }
    }
}

impl AsFlowEditorSource for SourceConfigSource {
    fn set_as_flow_sender(&mut self, sender: AsFlowSender<GenericItem, FactorioContext>) {
        self.sender = sender;
    }
}
