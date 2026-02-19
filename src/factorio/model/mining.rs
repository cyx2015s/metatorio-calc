use std::collections::HashSet;

use serde_with::serde_as;

use crate::{
    concept::{EntryOpRequest, EntryOpResult, Flow, SolveContext},
    factorio::{
        ModuleConfig, ModuleConfigEditor, ProjectContext, calc_quality_distribution,
        common::*,
        icon::Icon,
        modal::SelectorModal,
        model::{data::*, energy::*, entity::*, recipe::*},
        planner::FactoryContext,
        selector::Selector,
    },
    math::ElemVec,
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

#[serde_as]
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

    #[serde(default)]
    pub allowed_effects: Option<EffectTypeLimitation>,

    #[serde(default)]
    pub allowed_module_categories: Option<Vec<String>>,

    #[serde(default = "always_true")]
    pub uses_force_mining_productivity_bonus: bool,

    pub resource_drain_rate_percent: Option<f64>,
}

fn always_true() -> bool {
    true
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
#[serde(default)]
pub struct MiningInstance {
    pub resource: String,
    pub machine: IdWithQuality,
    pub module_config: ModuleConfig,
    pub fuel: Option<IdWithQuality>,
}

impl Default for MiningInstance {
    fn default() -> Self {
        MiningInstance {
            // TODO 不能保证 iron-ore 一定存在
            resource: "entity-unknown".to_string(),
            machine: ("entity-unknown".to_string(), 0).into(),
            module_config: ModuleConfig::default(),
            fuel: None,
        }
    }
}

impl SolveContext for MiningInstance {
    type Game = DataContext;
    type Item = GenericItem;
}

impl AsFlow for MiningInstance {
    fn as_flow(
        &self,
        data: &DataContext,
        proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> Flow<Self::Item> {
        let mut map = Flow::new();

        let mut module_effects = self.module_config.get_effect(data);

        let mut base_speed = 1.0;

        let quality_level = self.machine.1 as usize;

        let mut drain_rate = data.qualities[quality_level].mining_drill_resource_drain_multiplier();

        let miner = data.miners.get(&self.machine.0);

        if let Some(miner) = miner {
            module_effects = module_effects
                + miner
                    .effect_receiver
                    .clone()
                    .unwrap_or_default()
                    .base_effect
                    .clone();

            module_effects.productivity += if miner.uses_force_mining_productivity_bonus {
                proj.mining_productivity
            } else {
                0.0
            };
            module_effects = module_effects.clamped();
            base_speed = miner.mining_speed;
            // TODO: 确认游戏内的舍入方式
            drain_rate *= miner.resource_drain_rate_percent.unwrap_or(100.0) / 100.0;

            let energy_related_flow = energy_source_as_flow(
                data,
                &miner.energy_source,
                miner
                    .energy_usage
                    .as_ref()
                    .expect("MiningDrillPrototype 中的机器没有能量消耗"),
                &module_effects,
                &self
                    .fuel
                    .as_ref()
                    .map(|id_with_quality| (id_with_quality.0.clone(), id_with_quality.1 as i32)),
                &mut base_speed,
            );
            for (key, value) in energy_related_flow.into_iter() {
                index_map_update_entry(&mut map, key, value);
            }
        }

        let resource_ore = match data.resources.get(&self.resource) {
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

        index_map_update_entry(
            &mut map,
            GenericItem::Electricity,
            -self.module_config.get_consumption(data),
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
                temperature: [i32::MIN, i32::MAX],
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
            &data.qualities,
            module_effects.quality,
            0,
            proj.max_quality_level as usize,
        );
        {
            if let Some(result) = &mining_property.result {
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
            } else {
                for result in &mining_property.results {
                    let item = match result {
                        RecipeResult::Item(r) => {
                            GenericItem::Entity(IdWithQuality(r.name.clone(), 0))
                        }
                        RecipeResult::Fluid(r) => {
                            let default_temperature =
                                data.fluids
                                    .get(&r.name)
                                    .map(|f| f.default_temperature)
                                    .unwrap_or(15.0) as i32;
                            GenericItem::Fluid {
                                name: r.name.clone(),
                                temperature: [default_temperature, default_temperature],
                            }
                        }
                    };
                    match result {
                        RecipeResult::Item(r) => {
                            let (base_yield, extra_yield) = r.normalized_output();
                            let total_yield = base_speed
                                * (1.0 + module_effects.speed)
                                * (base_yield + extra_yield * module_effects.productivity);
                            for (quality_level, quality_prob) in
                                quality_distribution.iter().enumerate()
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
            }
        }
        map
    }

    fn cost(&self, data: &DataContext, _proj: &ProjectContext, _factory: &FactoryContext) -> f64 {
        if let Some(miner) = data.miners.get(&self.machine.0) {
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

#[test]
fn test_mining_normalized() {
    let data = DataContext::test_load();
    let proj = ProjectContext::default();
    let factory = FactoryContext::default();
    let mining_config = MiningInstance {
        resource: "uranium-ore".to_string(),
        machine: "big-mining-drill".into(),
        module_config: ModuleConfig::default(),
        fuel: None,
    };

    let result = mining_config.as_flow(&data, &proj, &factory);
    println!("Mining Result: {:?}", result);
    let result_with_location =
        crate::factorio::model::data::make_located_generic_recipe(result.clone(), 42);
    println!("Mining Result with Location: {:?}", result_with_location);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename = "factorio:mining", default)]
#[derive(Default)]
pub struct MiningMechanic {
    #[serde(skip)]
    pub operations: Vec<(usize, EntryOpRequest)>,
    pub instances: Vec<MiningInstance>,
    #[serde(skip)]
    pub suggestion_item: Option<GenericItem>,
    #[serde(skip)]
    pub suggestion_amount: f64,
    #[serde(skip)]
    pub suggested_resources: HashSet<String>,
    #[serde(skip)]
    pub selected_suggested_resource: Option<String>,
    #[serde(skip)]
    pub suggested_recipes_filter: String,
}

pub fn select_miner_for_resource(
    data: &DataContext,
    proj: &ProjectContext,
    _factory: &FactoryContext,
    resource: &ResourcePrototype,
    preferences: &[IdWithQuality],
) -> IdWithQuality {
    // 优先选择用户偏好
    for pref in preferences.iter() {
        if let Some(miner) = data.miners.get(&pref.0)
            && machine_fits_for_resource(miner, resource)
        {
            return pref.clone();
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
            score *= 1.0 + (effect_receiver.base_effect.productivity * 2.0);
        }
        score *= 1.0 + miner.module_slots;
        score
    }
    // 找不到偏好设定的机器，找一个最好的的
    for miner in data.miners.values() {
        if machine_fits_for_resource(miner, resource)
            && measure_miner(miner) > measure
            && proj.is_prototype_accessible("entity", &miner.base.base.name)
        {
            measure = measure_miner(miner);
            selected = miner.base.base.name.clone();
        }
    }
    selected.into()
}

impl SolveContext for MiningMechanic {
    type Game = DataContext;
    type Item = GenericItem;
}

#[typetag::serde(name = "factorio:mining")]
impl FactorioMechanic for MiningMechanic {
    fn name(&self) -> String {
        "采矿".to_string()
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
        if ui.button("添加采矿").clicked() {
            let mining_config = MiningInstance::default();
            self.instances.push(mining_config);
            changed = true;
        }
        changed
    }

    fn instance_view(
        &mut self,
        idx: usize,
        ui: &mut egui::Ui,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        let instance = &mut self.instances[idx];

        ui.vertical(|ui| {
            ui.label("开采");

            let resource_button = ui
                .add_sized([35.0, 35.0], Icon::new(data, "entity", &instance.resource))
                .interact(egui::Sense::click())
                .on_hover_text(format!(
                    "矿物：{}",
                    data.get_display_name("entity", &instance.resource)
                ));
            changed |= ui
                .add(
                    SelectorModal::new(resource_button.id, data, "选择矿物")
                        .with_toggle(resource_button.clicked())
                        .with_selector(
                            Selector::new(data, "entity")
                                .with_current(&mut instance.resource)
                                .with_filter(|s, f| f.resources.contains_key(s)),
                        ),
                )
                .changed();
        });
        if changed {
            // TODO 读取用户设定的偏好
            if let Some(resource) = data.resources.get(&instance.resource)
                && data
                    .miners
                    .get(&instance.machine.0)
                    .is_none_or(|miner| !machine_fits_for_resource(miner, resource))
            {
                instance.machine = select_miner_for_resource(data, proj, factory, resource, &[]);
                instance.fuel = None;
                instance.module_config = ModuleConfig::new();
            }
        }
        ui.separator();
        ui.vertical(|ui| {
            ui.label("采矿机");
            let entity_button = ui
                .add_sized(
                    [35.0, 35.0],
                    Icon::new(data, "entity", &instance.machine.0).with_quality(instance.machine.1),
                )
                .interact(egui::Sense::click())
                .on_hover_text(if data.miners.contains_key(&instance.machine.0) {
                    data.get_display_name("entity", &instance.machine.0)
                } else {
                    "采矿机: 未选择".into()
                });

            if let Some(resource_proto) = data.resources.get(&instance.resource) {
                changed |= ui
                    .add(
                        SelectorModal::new(entity_button.id, data, "选择采矿设备")
                            .with_toggle(entity_button.clicked())
                            .with_selector(
                                Selector::new(data, "entity")
                                    .with_current(&mut instance.machine)
                                    .with_filter(|s: &IdWithQuality, f: &DataContext| {
                                        if let Some(miner) = f.miners.get(&s.0) {
                                            machine_fits_for_resource(miner, resource_proto)
                                                && proj.is_prototype_accessible("entity", &s.0)
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

        if let Some(miner) = data.miners.get(&instance.machine.0) {
            let allowed_effects = Some(
                miner
                    .allowed_effects
                    .clone()
                    .unwrap_or(EffectTypeLimitation::new(true, true, true, true, true)),
            );
            changed |= ui
                .add(
                    ModuleConfigEditor::new(
                        data,
                        &mut instance.module_config,
                        miner.module_slots as usize,
                        &allowed_effects,
                        &miner.allowed_module_categories,
                    )
                    .with_project_context(proj),
                )
                .changed();
        }

        changed
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

    fn update_suggestion(
        &mut self,
        data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
        item: &GenericItem,
        amount: f64,
    ) {
        self.suggested_resources.clear();
        self.suggestion_item = Some(item.clone());
        self.suggestion_amount = amount;
        let value = amount;

        if value < 0.0 {
            // 提供生产方式
            match item {
                GenericItem::Item(IdWithQuality(name, _)) => {
                    for resource in data.resources.values() {
                        if let Some(mining) = resource.base.minable.as_ref() {
                            if let Some(result) = &mining.result {
                                if result == name {
                                    self.suggested_resources
                                        .insert(resource.base.base.name.clone());
                                }
                            } else {
                                for res in &mining.results {
                                    if let RecipeResult::Item(r) = res
                                        && &r.name == name
                                    {
                                        self.suggested_resources
                                            .insert(resource.base.base.name.clone());
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
                    for resource in data.resources.values() {
                        if let Some(mining) = resource.base.minable.as_ref() {
                            for res in &mining.results {
                                if let RecipeResult::Fluid(r) = res
                                    && &r.name == name
                                {
                                    self.suggested_resources
                                        .insert(resource.base.base.name.clone());
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

    fn suggestion_view(
        &mut self,
        ui: &mut egui::Ui,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        ui.add(egui::TextEdit::singleline(&mut self.suggested_recipes_filter).hint_text("筛选器"));
        ui.add(
            Selector::new(data, "entity")
                .with_output(&mut self.selected_suggested_resource)
                .with_filter(|id: &str, data| {
                    self.suggested_resources.contains(id)
                        && (id
                            .to_lowercase()
                            .contains(&self.suggested_recipes_filter.to_lowercase())
                            || data
                                .get_display_name("entity", id)
                                .to_lowercase()
                                .contains(&self.suggested_recipes_filter.to_lowercase()))
                }),
        );
        if let Some(resource) = &self.selected_suggested_resource {
            self.instances.push(MiningInstance {
                resource: resource.clone(),
                machine: select_miner_for_resource(
                    data,
                    proj,
                    factory,
                    data.resources.get(resource).unwrap(),
                    &[],
                ),
                ..Default::default()
            });
            self.selected_suggested_resource = None;
            changed = true;
        }
        changed
    }
    fn auto_populate(
        &mut self,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) {
        for resource in data.resources.values() {
            if let Some(_mining) = resource.base.minable.as_ref() {
                let machine = select_miner_for_resource(data, proj, factory, resource, &[]);
                self.instances.push(MiningInstance {
                    resource: resource.base.base.name.clone(),
                    machine,
                    ..Default::default()
                });
            }
        }
    }
}
