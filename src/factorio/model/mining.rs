use crate::{
    concept::{AsFlow, EditorView, Flow, Mechanic, MechanicProvider, MechanicSender, SolveContext},
    factorio::{
        ModuleConfig, ModuleConfigEditor, calc_quality_distribution,
        common::*,
        icon::Icon,
        modal::show_modal,
        model::{context::*, energy::*, entity::*, recipe::*},
        selector::ItemSelector,
    },
};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ResourcePrototype {
    #[serde(flatten)]
    pub base: EntityPrototype,

    pub category: Option<String>,

    #[serde(default)]
    pub infinite: bool,
}

impl HasPrototypeBase for ResourcePrototype {
    fn base(&self) -> &PrototypeBase {
        &self.base.base
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct MiningDrillPrototype {
    #[serde(flatten)]
    pub base: EntityPrototype,

    pub mining_speed: f64,

    pub resource_categories: Vec<String>,

    pub energy_source: EnergySource,
    #[serde(default)]
    pub energy_usage: Option<EnergyAmount>,
    #[serde(default)]
    pub effect_receiver: Option<EffectReceiver>,
    #[serde(default)]
    pub module_slots: f64,
    #[serde(default)]
    pub quality_affects_module_slots: bool,

    pub allowed_effects: Option<EffectTypeLimitation>,

    #[serde(deserialize_with = "option_as_vec_or_empty")]
    #[serde(default)]
    pub allowed_module_categories: Option<Vec<String>>,

    #[serde(default)]
    pub uses_force_mining_productivity_bonus: bool,

    pub resource_drain_rate_percent: Option<f64>,
}

impl HasPrototypeBase for MiningDrillPrototype {
    fn base(&self) -> &PrototypeBase {
        &self.base.base
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename = "factorio:mining")]
pub struct MiningConfig {
    pub resource: String,
    pub machine: Option<IdWithQuality>,
    pub module_config: ModuleConfig,
    pub instance_fuel: Option<IdWithQuality>,
}

impl Default for MiningConfig {
    fn default() -> Self {
        MiningConfig {
            // TODO 不能保证 iron-ore 一定存在
            resource: "iron-ore".to_string(),
            machine: None,
            module_config: ModuleConfig::default(),
            instance_fuel: None,
        }
    }
}

impl SolveContext for MiningConfig {
    type GameContext = FactorioContext;
    type ItemIdentType = GenericItem;
}

impl AsFlow for MiningConfig {
    fn as_flow(&self, ctx: &Self::GameContext) -> Flow<Self::ItemIdentType> {
        let mut map = Flow::new();

        let mut module_effects = self.module_config.get_effect(ctx).clamped();

        let mut base_speed = 1.0;

        let quality_level = self.machine.as_ref().map(|x| x.1).unwrap_or(0) as usize;

        let mut drain_rate = ctx.qualities[quality_level].mining_drill_resource_drain_multiplier();

        let miner = self.machine.as_ref().map(|machine_name| {
            ctx.miners
                .get(&machine_name.0)
                .expect("MiningConfig 中的机器在上下文中不存在")
        });

        if let Some(miner) = miner {
            module_effects = module_effects
                + miner
                    .effect_receiver
                    .clone()
                    .unwrap_or_default()
                    .base_effect
                    .clone();
            base_speed = miner.mining_speed;
            // TODO: 确认游戏内的舍入方式
            drain_rate *= miner.resource_drain_rate_percent.unwrap_or(100.0) / 100.0;

            let energy_related_flow = energy_source_as_flow(
                ctx,
                &miner.energy_source,
                miner
                    .energy_usage
                    .as_ref()
                    .expect("MiningDrillPrototype 中的机器没有能量消耗"),
                &module_effects,
                &self
                    .instance_fuel
                    .as_ref()
                    .map(|id_with_quality| (id_with_quality.0.clone(), id_with_quality.1 as i32)),
                &mut base_speed,
            );
            for (key, value) in energy_related_flow.into_iter() {
                index_map_update_entry(&mut map, key, value);
            }
        }

        let resource_ore = match ctx.resources.get(&self.resource) {
            Some(r) => r,
            None => return map,
        };

        if resource_ore.base.minable.is_none() {
            return map;
        }

        let mining_property = resource_ore.base.minable.as_ref().unwrap();

        base_speed /= mining_property.mining_time;

        // 计算矿物实体本身的消耗
        index_map_update_entry(
            &mut map,
            GenericItem::Entity {
                name: resource_ore.base.base.name.clone(),
                quality: 0,
            },
            -base_speed * (1.0 + module_effects.speed) * drain_rate,
        );

        // 计算开采液体的消耗
        if let Some(fluid) = resource_ore
            .base
            .minable
            .as_ref()
            .and_then(|m| m.required_fluid.clone())
        {
            let fluid_item = GenericItem::Fluid {
                name: fluid,
                temperature: None,
            };
            // TODO: 流体消耗受 drain_rate 影响吗？
            // 实际值还要除以 10
            let amount = base_speed
                * (1.0 + module_effects.speed)
                * mining_property
                    .fluid_amount
                    .expect("必须指定每次开采的流体消耗")
                / 10.0;

            index_map_update_entry(&mut map, fluid_item, -amount);
        }
        let quality_distribution = calc_quality_distribution(
            &ctx.qualities,
            module_effects.quality,
            0,
            ctx.qualities.len(),
        );
        if let Some(results) = &mining_property.results {
            for result in results.iter() {
                let item = match result {
                    RecipeResult::Item(r) => GenericItem::Entity {
                        name: r.name.clone(),
                        quality: 0,
                    },
                    RecipeResult::Fluid(r) => GenericItem::Fluid {
                        name: r.name.clone(),
                        temperature: None,
                    },
                };
                match result {
                    RecipeResult::Item(r) => {
                        let (base_yield, extra_yield) = r.normalized_output();
                        let total_yield = base_speed
                            * (1.0 + module_effects.speed)
                            * (base_yield + extra_yield * module_effects.productivity);
                        for (quality_level, quality_prob) in quality_distribution.iter().enumerate()
                        {
                            if *quality_prob > 0.0 {
                                index_map_update_entry(
                                    &mut map,
                                    GenericItem::Item {
                                        name: r.name.clone(),
                                        quality: quality_level as u8,
                                    },
                                    total_yield * *quality_prob,
                                );
                            }
                        }
                    }
                    RecipeResult::Fluid(r) => {
                        let (base_yield, extra_yield) = r.normalized_output();
                        index_map_update_entry(
                            &mut map,
                            item,
                            base_speed
                                * (1.0 + module_effects.speed)
                                * (base_yield + extra_yield * module_effects.productivity),
                        );
                    }
                };
            }
        } else {
            let result = mining_property
                .result
                .as_ref()
                .expect("results or result must exist");
            let count = mining_property.count.unwrap_or(1.0);
            let total_yield = base_speed
                * (1.0 + module_effects.speed)
                * count
                * (1.0 + module_effects.productivity);
            for (quality_level, quality_prob) in quality_distribution.iter().enumerate() {
                if *quality_prob > 0.0 {
                    index_map_update_entry(
                        &mut map,
                        GenericItem::Item {
                            name: result.clone(),
                            quality: quality_level as u8,
                        },
                        total_yield * *quality_prob,
                    );
                }
            }
        }
        map
    }

    fn cost(&self, ctx: &Self::GameContext) -> f64 {
        if let Some(machine) = &self.machine {
            let miner = ctx.miners.get(&machine.0).unwrap();
            miner
                .base
                .collision_box
                .as_ref()
                .map_or(1.0, |bounding_box| match bounding_box {
                    BoundingBox::Struct {
                        left_top,
                        right_bottom,
                        orientation: _,
                    } => {
                        f64::ceil(right_bottom.1 - left_top.1)
                            * f64::ceil(right_bottom.0 - left_top.0)
                    }
                    BoundingBox::Pair(map_position, map_position1) => {
                        f64::ceil(map_position1.1 - map_position.1)
                            * f64::ceil(map_position1.0 - map_position.0)
                    }
                    BoundingBox::Triplet(map_position, map_position1, _) => {
                        f64::ceil(map_position1.1 - map_position.1)
                            * f64::ceil(map_position1.0 - map_position.0)
                    }
                })
        } else {
            16.0
        }
    }
}

impl EditorView for MiningConfig {
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &Self::GameContext) {
        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.label("开采");

                let resource_button = ui
                    .add_sized(
                        [35.0, 35.0],
                        Icon {
                            ctx,
                            type_name: "entity",
                            item_name: &self.resource,
                            quality: 0,
                            size: 32.0,
                        },
                    )
                    .interact(egui::Sense::click())
                    .on_hover_text(format!(
                        "矿物：{}",
                        ctx.get_display_name("entity", &self.resource)
                    ));
                show_modal(resource_button.id, resource_button.clicked(), ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_width(f32::INFINITY)
                        .auto_shrink(false)
                        .show(ui, |ui| {
                            ui.add(
                                ItemSelector::new(ctx, "entity")
                                    .with_current(&mut self.resource)
                                    .with_filter(|s: &str, f| f.resources.contains_key(s)),
                            );
                        });
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                let entity_button = if let Some(machine) = &self.machine {
                    ui.add_sized(
                        [35.0, 35.0],
                        Icon {
                            ctx,
                            type_name: "entity",
                            item_name: &machine.0,
                            quality: machine.1,
                            size: 32.0,
                        },
                    )
                    .interact(egui::Sense::click())
                    .on_hover_text(ctx.get_display_name("entity", &machine.0))
                } else {
                    ui.add_sized(
                        [35.0, 35.0],
                        Icon {
                            ctx,
                            type_name: "entity",
                            item_name: "entity-unknown",
                            quality: 0,
                            size: 32.0,
                        },
                    )
                    .interact(egui::Sense::click())
                    .on_hover_text("采矿机：未选择")
                };
                let resource_proto = ctx
                    .resources
                    .get(&self.resource)
                    .expect("MiningConfig 中的矿物在上下文中不存在");
                let mut selected_entity = None;
                show_modal(entity_button.id, entity_button.clicked(), ui, |ui| {
                    ui.label("选择机器");
                    egui::ScrollArea::vertical()
                        .max_width(f32::INFINITY)
                        .auto_shrink(false)
                        .show(ui, |ui| {
                            ui.add(
                                ItemSelector::new(ctx, "entity")
                                    .with_output(&mut selected_entity)
                                    .with_filter(|s, f| {
                                        if let Some(miner) = f.miners.get(s) {
                                            miner.resource_categories.contains(
                                                resource_proto
                                                    .category
                                                    .as_ref()
                                                    .unwrap_or(&"basic-solid".to_string()),
                                            )
                                        } else {
                                            false
                                        }
                                    }),
                            );
                        });

                    if selected_entity.is_some() {
                        ui.close();
                    }

                    if let Some(selected_entity) = selected_entity {
                        self.machine = Some(
                            (selected_entity, self.machine.as_ref().map_or(0, |m| m.1)).into(),
                        );
                    }
                });
            });
            ui.separator();

            if let Some(miner) = self
                .machine
                .as_ref()
                .and_then(|machine| ctx.miners.get(&machine.0))
            {
                ui.add(ModuleConfigEditor::new(
                    ctx,
                    &mut self.module_config,
                    miner.module_slots as usize,
                    &miner.allowed_effects,
                    &miner.allowed_module_categories,
                ));
            }
        });
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MiningConfigProvider {
    #[serde(skip)]
    pub sender: Option<MechanicSender<GenericItem, FactorioContext>>,
}

impl Default for MiningConfigProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MiningConfigProvider {
    pub fn new() -> Self {
        Self { sender: None }
    }
}

impl SolveContext for MiningConfigProvider {
    type GameContext = FactorioContext;
    type ItemIdentType = GenericItem;
}

impl EditorView for MiningConfigProvider {
    fn editor_view(&mut self, ui: &mut egui::Ui, _ctx: &Self::GameContext) {
        if ui.button("添加采矿").clicked() {
            let mining_config = MiningConfig::default();
            if let Some(sender) = &self.sender {
                let _ = sender.send(Box::new(mining_config));
            }
        }
    }
}

impl MechanicProvider for MiningConfigProvider {
    fn set_mechanic_sender(
        &mut self,
        sender: MechanicSender<Self::ItemIdentType, Self::GameContext>,
    ) {
        self.sender = Some(sender);
    }

    fn hint_populate(
        &self,
        ctx: &Self::GameContext,
        item: &Self::ItemIdentType,
        value: f64,
    ) -> Vec<Box<dyn Mechanic<ItemIdentType = Self::ItemIdentType, GameContext = Self::GameContext>>>
    {
        let mut ret = vec![];
        if value < 0.0 {
            // 提供生产方式
            match item {
                GenericItem::Item { name, quality: _ } => {
                    for resource in ctx.resources.values() {
                        if let Some(mining) = resource.base.minable.as_ref() {
                            if let Some(result) = &mining.result {
                                if result == name {
                                    let mining_config = MiningConfig {
                                        resource: resource.base.base.name.clone(),
                                        machine: None,
                                        module_config: ModuleConfig::default(),
                                        instance_fuel: None,
                                    };
                                    ret.push(Box::new(mining_config)
                                        as Box<
                                            dyn Mechanic<
                                                    ItemIdentType = GenericItem,
                                                    GameContext = FactorioContext,
                                                >,
                                        >);
                                }
                            } else {
                                for res in mining.results.as_ref().unwrap().iter() {
                                    if let RecipeResult::Item(r) = res
                                        && &r.name == name
                                    {
                                        let mining_config = MiningConfig {
                                            resource: resource.base.base.name.clone(),
                                            machine: None,
                                            module_config: ModuleConfig::default(),
                                            instance_fuel: None,
                                        };
                                        ret.push(Box::new(mining_config)
                                            as Box<
                                                dyn Mechanic<
                                                        ItemIdentType = GenericItem,
                                                        GameContext = FactorioContext,
                                                    >,
                                            >);
                                    }
                                }
                            }
                        }
                    }
                }
                GenericItem::Fluid {
                    name,
                    temperature: _,
                } => {
                    for resource in ctx.resources.values() {
                        if let Some(mining) = resource.base.minable.as_ref()
                            && let Some(results) = &mining.results
                        {
                            for res in results.iter() {
                                if let RecipeResult::Fluid(r) = res
                                    && &r.name == name
                                {
                                    let mining_config = MiningConfig {
                                        resource: resource.base.base.name.clone(),
                                        machine: None,
                                        module_config: ModuleConfig::default(),
                                        instance_fuel: None,
                                    };
                                    ret.push(Box::new(mining_config)
                                        as Box<
                                            dyn Mechanic<
                                                    ItemIdentType = GenericItem,
                                                    GameContext = FactorioContext,
                                                >,
                                        >);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        } else {
            // TODO 提供消耗方式
        }
        ret
    }
}

#[test]
fn test_mining_normalized() {
    let ctx = FactorioContext::default();
    let mining_config = MiningConfig {
        resource: "uranium-ore".to_string(),
        machine: Some(("big-mining-drill".to_string(), 0).into()),
        module_config: ModuleConfig::default(),
        instance_fuel: None,
    };

    let result = mining_config.as_flow(&ctx);
    println!("Mining Result: {:?}", result);
    let result_with_location =
        crate::factorio::model::context::make_located_generic_recipe(result.clone(), 42);
    println!("Mining Result with Location: {:?}", result_with_location);
}

crate::impl_register_deserializer!(
    for MiningConfig
    as "factorio:mining"
    => dyn Mechanic<ItemIdentType = GenericItem, GameContext = FactorioContext>
);
