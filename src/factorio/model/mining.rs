use std::collections::VecDeque;

use crate::{
    concept::{AsFlow, EditorView, Flow, Mechanic, SolveContext, VecItemOp},
    factorio::{
        ModuleConfig, ModuleConfigEditor, calc_quality_distribution,
        common::*,
        icon::Icon,
        modal::SelectorModal,
        model::{context::*, energy::*, entity::*, recipe::*},
        selector::Selector,
        style::card_frame,
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

pub fn machine_fits_for_resource(
    miner: &MiningDrillPrototype,
    resource: &ResourcePrototype,
) -> bool {
    miner.resource_categories.contains(
        resource
            .category
            .as_ref()
            .unwrap_or(&"basic-solid".to_string()),
    )
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename = "factorio:mining")]
pub struct MiningMechanicInstance {
    pub resource: String,
    pub machine: IdWithQuality,
    pub module_config: ModuleConfig,
    pub instance_fuel: Option<IdWithQuality>,
}

impl Default for MiningMechanicInstance {
    fn default() -> Self {
        MiningMechanicInstance {
            // TODO 不能保证 iron-ore 一定存在
            resource: "entity-unknown".to_string(),
            machine: ("entity-unknown".to_string(), 0).into(),
            module_config: ModuleConfig::default(),
            instance_fuel: None,
        }
    }
}

impl SolveContext for MiningMechanicInstance {
    type GameContext = FactorioContext;
    type ItemIdentType = GenericItem;
}

impl AsFlow for MiningMechanicInstance {
    fn as_flow(&self, ctx: &Self::GameContext) -> Flow<Self::ItemIdentType> {
        let mut map = Flow::new();

        let mut module_effects = self.module_config.get_effect(ctx).clamped();

        let mut base_speed = 1.0;

        let quality_level = self.machine.1 as usize;

        let mut drain_rate = ctx.qualities[quality_level].mining_drill_resource_drain_multiplier();

        let miner = ctx.miners.get(&self.machine.0);

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
            GenericItem::Entity(IdWithQuality(resource_ore.base.base.name.clone(), 0)),
            -base_speed * (1.0 + module_effects.speed) * drain_rate,
        );

        // 计算开采流体的消耗
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
                    RecipeResult::Item(r) => GenericItem::Entity(IdWithQuality(r.name.clone(), 0)),
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
                                    GenericItem::Item(IdWithQuality(
                                        r.name.clone(),
                                        quality_level as u8,
                                    )),
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
                        GenericItem::Item(IdWithQuality(result.clone(), quality_level as u8)),
                        total_yield * *quality_prob,
                    );
                }
            }
        }
        map
    }

    fn cost(&self, ctx: &Self::GameContext) -> f64 {
        if let Some(miner) = ctx.miners.get(&self.machine.0) {
            miner
                .base
                .collision_box
                .as_ref()
                .map_or(1.0, |bounding_box| bounding_box.get_area())
        } else {
            16.0
        }
    }
}

impl EditorView for MiningMechanicInstance {
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &Self::GameContext) -> bool {
        let mut changed = false;

        ui.vertical(|ui| {
            ui.label("开采");

            let resource_button = ui
                .add_sized([35.0, 35.0], Icon::new(ctx, "entity", &self.resource))
                .interact(egui::Sense::click())
                .on_hover_text(format!(
                    "矿物：{}",
                    ctx.get_display_name("entity", &self.resource)
                ));
            changed |= ui
                .add(
                    SelectorModal::new(resource_button.id, ctx, "选择矿物")
                        .with_toggle(resource_button.clicked())
                        .with_selector(
                            Selector::new(ctx, "entity")
                                .with_current(&mut self.resource)
                                .with_filter(|s, f| f.resources.contains_key(s)),
                        ),
                )
                .changed();
        });
        if changed {
            // TODO 读取用户设定的偏好
            if let Some(miner) = ctx.miners.get(&self.machine.0)
                && let Some(resource_proto) = ctx.resources.get(&self.resource)
                && !machine_fits_for_resource(miner, resource_proto)
            {
                self.machine = "entity-unknown".into();
                self.instance_fuel = None;
                self.module_config = ModuleConfig::new();
            }
        }
        ui.separator();
        ui.vertical(|ui| {
            ui.add_sized([35.0, 15.0], egui::Label::new("机器"));
            let entity_button = ui
                .add_sized(
                    [35.0, 35.0],
                    Icon::new(ctx, "entity", &self.machine.0).with_quality(self.machine.1),
                )
                .interact(egui::Sense::click())
                .on_hover_text(if ctx.miners.contains_key(&self.machine.0) {
                    ctx.get_display_name("entity", &self.machine.0)
                } else {
                    "采矿机: 未选择".into()
                });

            if let Some(resource_proto) = ctx.resources.get(&self.resource) {
                changed |= ui
                    .add(
                        SelectorModal::new(entity_button.id, ctx, "选择采矿设备")
                            .with_toggle(entity_button.clicked())
                            .with_selector(
                                Selector::new(ctx, "entity")
                                    .with_current(&mut self.machine)
                                    .with_filter(|s: &IdWithQuality, f: &FactorioContext| {
                                        if let Some(miner) = f.miners.get(&s.0) {
                                            machine_fits_for_resource(miner, resource_proto)
                                        } else {
                                            false
                                        }
                                    }),
                            ),
                    )
                    .changed();
            }
        });
        ui.separator();

        if let Some(miner) = ctx.miners.get(&self.machine.0) {
            changed |= ui
                .add(ModuleConfigEditor::new(
                    ctx,
                    &mut self.module_config,
                    miner.module_slots as usize,
                    &miner.allowed_effects,
                    &miner.allowed_module_categories,
                ))
                .changed();
        }

        // 先不判断
        changed
    }
}

#[test]
fn test_mining_normalized() {
    let ctx = FactorioContext::test_load();
    let mining_config = MiningMechanicInstance {
        resource: "uranium-ore".to_string(),
        machine: "big-mining-drill".into(),
        module_config: ModuleConfig::default(),
        instance_fuel: None,
    };

    let result = mining_config.as_flow(&ctx);
    println!("Mining Result: {:?}", result);
    let result_with_location =
        crate::factorio::model::context::make_located_generic_recipe(result.clone(), 42);
    println!("Mining Result with Location: {:?}", result_with_location);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename = "factorio:mining", default)]
#[derive(Default)]
pub struct MiningMechanic {
    pub instances: Vec<MiningMechanicInstance>,
    #[serde(skip)]
    pub suggestions: Vec<MiningMechanicInstance>,
}

pub fn select_miner_for_resource(
    ctx: &FactorioContext,
    resource: &ResourcePrototype,
    preferences: &[IdWithQuality],
) -> IdWithQuality {
    // 优先选择用户偏好
    for pref in preferences.iter() {
        if let Some(miner) = ctx.miners.get(&pref.0) {
            if machine_fits_for_resource(miner, resource) {
                return pref.clone();
            }
        }
    }
    let mut measure = 0.0;
    let mut selected = "entity-unknown".to_string();
    fn measure_miner(miner: &MiningDrillPrototype) -> f64 {
        let mut score = miner.mining_speed
            / miner
                .base
                .collision_box
                .as_ref()
                .map_or(25.0, |bb| bb.get_area());
        if let Some(effect_receiver) = &miner.effect_receiver {
            score *= 1.0 + effect_receiver.base_effect.speed;
            score *= 1.0 + effect_receiver.base_effect.productivity;
        }
        score
    }
    // 找不到偏好设定的机器，找一个最好的的
    for miner in ctx.miners.values() {
        if machine_fits_for_resource(miner, resource) && measure_miner(miner) > measure {
            measure = measure_miner(miner);
            selected = miner.base.base.name.clone();
        }
    }
    selected.into()
}

impl SolveContext for MiningMechanic {
    type GameContext = FactorioContext;
    type ItemIdentType = GenericItem;
}

impl Mechanic<FactorioContext, GenericItem> for MiningMechanic {
    fn name(&self) -> String {
        "采矿".to_string()
    }

    fn instances(&self) -> Vec<&FactorioMechanicInstance> {
        self.instances
            .iter()
            .map(|instance| instance as &FactorioMechanicInstance)
            .collect()
    }

    fn instances_mut(&mut self) -> Vec<&mut FactorioMechanicInstance> {
        self.instances
            .iter_mut()
            .map(|instance| instance as &mut FactorioMechanicInstance)
            .collect()
    }

    fn instances_operate(&mut self, f: &mut dyn FnMut(&mut FactorioMechanicInstance) -> VecItemOp) {
        let mut retain = VecDeque::new();
        retain.reserve(self.instances.len());
        let mut new_items = Vec::new();
        for instance in self.instances.iter_mut() {
            let op = f(instance as &mut FactorioMechanicInstance);
            match op {
                VecItemOp::None => {
                    retain.push_back(true);
                }
                VecItemOp::Drop => {
                    retain.push_back(false);
                }
                VecItemOp::Clone => {
                    retain.push_back(true);
                    new_items.push(instance.clone());
                }
            }
        }
        self.instances
            .retain_mut(|_| retain.pop_front().unwrap_or(true));
        self.instances.extend(new_items);
    }

    fn update_suggestion(&mut self, ctx: &FactorioContext, item: &GenericItem, amount: f64) {
        self.suggestions.clear();
        let value = amount;
        if value < 0.0 {
            // 提供生产方式
            match item {
                GenericItem::Item(IdWithQuality(name, _)) => {
                    for resource in ctx.resources.values() {
                        if let Some(mining) = resource.base.minable.as_ref() {
                            if let Some(result) = &mining.result {
                                if result == name {
                                    let mut mining_config = MiningMechanicInstance {
                                        resource: resource.base.base.name.clone(),
                                        ..Default::default()
                                    };
                                    for miner in ctx.miners.values() {
                                        if miner.resource_categories.contains(
                                            resource
                                                .category
                                                .as_ref()
                                                .unwrap_or(&"basic-solid".to_string()),
                                        ) {
                                            mining_config.machine =
                                                (miner.base.base.name.clone(), 0).into();
                                            break;
                                        }
                                    }
                                    self.suggestions.push(mining_config);
                                }
                            } else {
                                for res in mining.results.as_ref().unwrap().iter() {
                                    if let RecipeResult::Item(r) = res
                                        && &r.name == name
                                    {
                                        let mining_config = MiningMechanicInstance {
                                            resource: resource.base.base.name.clone(),
                                            machine: "entity-unknown".into(),
                                            module_config: ModuleConfig::default(),
                                            instance_fuel: None,
                                        };
                                        self.suggestions.push(mining_config);
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
                                    let mining_config = MiningMechanicInstance {
                                        resource: resource.base.base.name.clone(),
                                        machine: "entity-unknown".into(),
                                        module_config: ModuleConfig::default(),
                                        instance_fuel: None,
                                    };
                                    self.suggestions.push(mining_config);
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
    }

    fn suggestion_view(&mut self, ui: &mut egui::Ui, ctx: &FactorioContext) -> bool {
        let mut changed = false;
        self.suggestions.iter_mut().for_each(|instance| {
            card_frame(ui).show(ui, |ui| {
                if ui.button("添加此配方").clicked() {
                    self.instances.push(instance.clone());
                    changed = true;
                }
                ui.horizontal(|ui| {
                    ui.set_min_width(ui.available_width());
                    instance.editor_view(ui, ctx);
                });
            });
        });
        changed
    }
}

impl EditorView for MiningMechanic {
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &Self::GameContext) -> bool {
        let _ = ctx;
        if ui.button("添加采矿").clicked() {
            let mining_config = MiningMechanicInstance::default();
            self.instances.push(mining_config);
            return true;
        }
        false
    }
}

crate::impl_register_deserializer!(
    for MiningMechanic
    as "factorio:mining"
    => dyn Mechanic<FactorioContext, GenericItem>
);
