use crate::{
    concept::{EntryOpRequest, EntryOpResult, Flow, SolveContext},
    factorio::{
        DataContext, EntityPrototype, GenericItem, ProjectContext, common::*,
        energy_source_as_flow, hover::PrototypeHover, icon::Icon, modal::SelectorModal,
        planner::FactoryContext, selector::Selector,
    },
    math::{ElemVec, flow_add},
};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FluidPrototype {
    #[serde(flatten)]
    pub base: PrototypeBase,

    pub default_temperature: f64,

    pub max_temperature: Option<f64>,

    /// 一单位流体上升一摄氏度所需的能量
    pub heat_capacity: Option<EnergyAmount>,

    /// 燃烧每单位流体所释放的能量
    pub fuel_value: Option<EnergyAmount>,
}

impl HasPrototypeBase for FluidPrototype {
    fn base(&self) -> &PrototypeBase {
        &self.base
    }
}

fn one() -> f64 {
    1.0
}

fn always_true() -> bool {
    true
}
#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(default)]
pub struct GeneratorPrototype {
    #[serde(flatten)]
    pub base: EntityPrototype,

    pub energy_source: ElectricEnergySource,

    pub fluid_box: FluidBox,

    #[serde(default = "one")]
    pub effectivity: f64,

    fluid_usage_per_tick: f64,

    pub maximum_temperature: f64,

    pub max_power_output: Option<EnergyAmount>,

    pub scale_fluid_usage: bool,

    pub burns_fluid: bool,
    #[serde(default = "always_true")]
    pub destroy_non_fuel_fluid: bool,
}

impl GeneratorPrototype {
    // 输入指定温度的流体时，流体的消耗量和电量产出
    // 返回：(每秒消耗的流体量, 每秒产生的电量)
    pub fn get_output(&self, fluid: &FluidPrototype, temperature: f64) -> (f64, f64) {
        let mut scale = 1.0;
        if self.burns_fluid {
            // 直接燃烧流体产生电力的发电机
            let fuel_value = fluid.fuel_value.unwrap_or_default();
            let actual_power_output = EnergyAmount {
                amount: self.fluid_usage_per_tick * fuel_value.amount * self.effectivity,
            };
            if self.scale_fluid_usage
                && let Some(max_power_output) = self.max_power_output
            {
                if actual_power_output > max_power_output {
                    scale = max_power_output.amount / actual_power_output.amount;
                    return (
                        self.fluid_usage_per_tick * scale * 60.0,
                        max_power_output.amount * 60.0,
                    );
                }
                (
                    self.fluid_usage_per_tick * scale * 60.0,
                    actual_power_output.amount * 60.0,
                )
            } else {
                if let Some(max_power_output) = self.max_power_output
                    && actual_power_output > max_power_output
                {
                    return (
                        self.fluid_usage_per_tick * 60.0,
                        max_power_output.amount * 60.0,
                    );
                }
                (
                    self.fluid_usage_per_tick * 60.0,
                    actual_power_output.amount * 60.0,
                )
            }
        } else {
            // 靠热量差产生电力的发电机
            let heat_capacity = fluid
                .heat_capacity
                .unwrap_or(EnergyAmount { amount: 1000.0 });
            let max_power_output = if let Some(max_power_output) = self.max_power_output {
                max_power_output
            } else {
                let filter = self.fluid_box.filter.as_ref().unwrap();
                if &fluid.base.name != filter {
                    // 如果流体不符合过滤条件，则不产生电力
                    if self.destroy_non_fuel_fluid {
                        return (self.fluid_usage_per_tick * 60.0, 0.0);
                    } else {
                        return (0.0, 0.0);
                    }
                }
                let max_temperature = if let Some(max_temperature) = fluid.max_temperature {
                    max_temperature.min(self.maximum_temperature)
                } else {
                    self.maximum_temperature
                };
                let temperature_diff = max_temperature - fluid.default_temperature;
                EnergyAmount {
                    amount: temperature_diff
                        * self.fluid_usage_per_tick
                        * heat_capacity.amount
                        * self.effectivity,
                }
            };
            let actual_power_output = EnergyAmount {
                amount: (temperature - fluid.default_temperature)
                    * self.fluid_usage_per_tick
                    * heat_capacity.amount
                    * self.effectivity,
            };

            if self.scale_fluid_usage {
                if actual_power_output > max_power_output {
                    scale = max_power_output.amount / actual_power_output.amount;
                    return (
                        self.fluid_usage_per_tick * scale * 60.0,
                        max_power_output.amount * 60.0,
                    );
                }
                (
                    self.fluid_usage_per_tick * scale * 60.0,
                    actual_power_output.amount * 60.0,
                )
            } else {
                if actual_power_output > max_power_output {
                    return (
                        self.fluid_usage_per_tick * 60.0,
                        max_power_output.amount * 60.0,
                    );
                }
                (
                    self.fluid_usage_per_tick * 60.0,
                    actual_power_output.amount * 60.0,
                )
            }
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]

pub struct BoilerPrototype {
    #[serde(flatten)]
    pub base: EntityPrototype,

    pub energy_source: EnergySource,

    pub energy_consumption: EnergyAmount,

    #[serde(default)]
    pub fluid_box: FluidBox,
    #[serde(default)]
    pub output_fluid_box: FluidBox,
    #[serde(default)]
    pub target_temperature: Option<f64>,
    #[serde(default)]
    pub mode: BoilerMode,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BoilerMode {
    #[default]
    HeatFluidInside,
    OutputToSeparatePipe,
}

impl BoilerPrototype {
    // 在输入指定流体和指定燃料时，对应的流状况
    pub fn get_flow(
        &self,
        data: &DataContext,
        fluid: &String,
        temperature: f64,
        fuel: &Option<(String, i32)>,
    ) -> Flow<GenericItem> {
        let mut flow = Flow::new();
        let mut fulfillment = 1.0;
        if self.fluid_box.filter.as_ref().is_some_and(|f| f != fluid) {
            return flow;
        }
        flow = flow_add(
            &flow,
            &energy_source_as_flow(
                data,
                &self.energy_source,
                &self.energy_consumption,
                &Effect::default(),
                fuel,
                &mut fulfillment,
            ),
            1.0,
        );
        match self.mode {
            BoilerMode::HeatFluidInside => {
                // TODO 在锅炉内部将流体加热
                // 这个加热是连续的过程，需要用户指定加热到多少度，才能计算出流体的消耗量和产出量
                // 考虑到原版没有直接读取管道内流体温度的机制，实际上玩家也无法控制吧？
                // 暂时先不实现这个模式了，等以后有需要再说
                // todo!();
            }
            BoilerMode::OutputToSeparatePipe => {
                let source_fluid = data.fluids.get(fluid).expect("锅炉输入的流体不存在");
                let source_heat_capacity = source_fluid
                    .heat_capacity
                    .unwrap_or(EnergyAmount { amount: 1000.0 });
                let target_fluid_name = self
                    .output_fluid_box
                    .filter
                    .clone()
                    .unwrap_or(fluid.clone());
                let target_fluid = data
                    .fluids
                    .get(&target_fluid_name)
                    .expect("锅炉输出的流体不存在");
                let target_heat_capacity = target_fluid
                    .heat_capacity
                    .unwrap_or(EnergyAmount { amount: 1000.0 });
                let target_fluid_temperature =
                    self.target_temperature.expect("锅炉没有指定目标温度");
                let amount = self.energy_consumption.amount * 60.0 // 功率
                    / source_heat_capacity.amount // 输入流体的比热容
                    / (target_fluid_temperature - temperature); // 温度差
                index_map_update_entry(
                    &mut flow,
                    GenericItem::Fluid {
                        name: fluid.clone(),
                        temperature: [temperature as i32, temperature as i32],
                    },
                    -amount,
                );
                index_map_update_entry(
                    &mut flow,
                    GenericItem::Fluid {
                        name: target_fluid_name,
                        temperature: [
                            target_fluid_temperature as i32,
                            target_fluid_temperature as i32,
                        ],
                    },
                    // https://lua-api.factorio.com/2.0.75/prototypes/BoilerPrototype.html#mode
                    amount * source_heat_capacity.amount / target_heat_capacity.amount,
                );
            }
        }
        flow
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GeneratorMechanic {
    #[serde(skip)]
    pub operations: Vec<(usize, EntryOpRequest)>,

    pub instances: Vec<GeneratorInstance>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GeneratorInstance {
    pub generator: IdWithQuality,

    pub fluid: String,

    pub temperature: i32,
}

impl SolveContext for GeneratorInstance {
    type Game = DataContext;
    type Item = GenericItem;
}

impl AsFlow for GeneratorInstance {
    fn as_flow(
        &self,
        data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> Flow<GenericItem> {
        let mut flow = Flow::new();
        if let Some(generator) = data.generators.get(&self.generator.0) {
            if let Some(filter) = generator.fluid_box.filter.as_ref() {
                let fluid = data.fluids.get(filter).expect("发电机输入的流体不存在");
                let (fluid_usage, power_output) =
                    generator.get_output(fluid, self.temperature as f64);
                index_map_update_entry(
                    &mut flow,
                    GenericItem::Fluid {
                        name: filter.clone(),
                        temperature: [self.temperature, self.temperature],
                    },
                    -fluid_usage,
                );
                index_map_update_entry(&mut flow, GenericItem::Electricity, power_output);
            } else if let Some(fluid) = data.fluids.get(&self.fluid) {
                let (fluid_usage, power_output) =
                    generator.get_output(fluid, self.temperature as f64);
                index_map_update_entry(
                    &mut flow,
                    GenericItem::Fluid {
                        name: self.fluid.clone(),
                        temperature: [self.temperature, self.temperature],
                    },
                    -fluid_usage,
                );
                index_map_update_entry(&mut flow, GenericItem::Electricity, power_output);
            }
        }
        let idx = (self.generator.1 as usize).min(data.qualities.len() - 1);
        let multiplier = data.qualities[idx].default_multiplier();
        flow.iter_mut().for_each(|v| *v.1 *= multiplier);
        flow
    }

    fn cost(&self, data: &DataContext, _proj: &ProjectContext, _factory: &FactoryContext) -> f64 {
        if let Some(generator) = data.generators.get(&self.generator.0) {
            generator
                .base
                .collision_box
                .as_ref()
                .map_or(16.0, |b| b.get_area())
        } else {
            16.0
        }
    }
}

impl SolveContext for GeneratorMechanic {
    type Game = DataContext;
    type Item = GenericItem;
}

#[typetag::serde(name = "factorio:generator")]
impl FactorioMechanic for GeneratorMechanic {
    fn name(&self) -> String {
        "流体发电".to_string()
    }

    fn editor_view(
        &mut self,
        ui: &mut egui::Ui,
        _data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        if ui.button("添加流体发电").clicked() {
            let new_config = GeneratorInstance {
                generator: "entity-unknown".into(),

                fluid: "fluid-unknown".to_string(),
                temperature: 25,
            };
            self.instances.push(new_config);
            changed = true;
        }
        changed
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

    fn instance_view(
        &mut self,
        idx: usize,
        ui: &mut egui::Ui,
        data: &DataContext,
        proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        let instance = &mut self.instances[idx];
        ui.vertical(|ui| {
            ui.label("发电机");
            let entity_button = ui
                .add_sized(
                    [35.0, 35.0],
                    Icon::new(data, "entity", &instance.generator.0)
                        .with_quality(instance.generator.1),
                )
                .interact(egui::Sense::click());
            if ui
                .add(
                    SelectorModal::new(entity_button.id, data, "选择发电机")
                        .with_toggle(entity_button.clicked())
                        .with_selector(
                            Selector::new(data, "entity")
                                .with_current(&mut instance.generator)
                                .with_filter(|s: &IdWithQuality, f| {
                                    f.generators.contains_key(&s.0)
                                        && proj.is_prototype_accessible("entity", &s.0)
                                }),
                        ),
                )
                .changed()
            {
                changed = true;
                instance.temperature =
                    data.generators[&instance.generator.0].maximum_temperature as i32;
            }
        });
        ui.separator();
        if let Some(generator) = data.generators.get(&instance.generator.0) {
            if let Some(filter) = &generator.fluid_box.filter {
                // 如果发电机指定了输入流体，则显示这个流体
                ui.vertical(|ui| {
                    ui.label("固定输入");
                    ui.add_sized([35.0, 35.0], Icon::new(data, "fluid", filter));
                });
            } else {
                // 如果发电机没有指定输入流体，则允许用户选择输入流体
                ui.vertical(|ui| {
                    ui.label("编辑输入");
                    let fluid_button = ui
                        .add_sized([35.0, 35.0], Icon::new(data, "fluid", &instance.fluid))
                        .interact(egui::Sense::click());
                    if ui
                        .add(
                            SelectorModal::new(fluid_button.id, data, "选择输入流体")
                                .with_toggle(fluid_button.clicked())
                                .with_selector(
                                    Selector::new(data, "fluid")
                                        .with_current(&mut instance.fluid)
                                        .with_filter(|s: &str, f| {
                                            if let Some(fluid_prototype) = f.fluids.get(s) {
                                                return proj.is_prototype_accessible("fluid", s)
                                                    && ((generator.burns_fluid
                                                        && fluid_prototype.fuel_value.is_some())
                                                        || (!generator.burns_fluid
                                                            && fluid_prototype
                                                                .heat_capacity
                                                                .is_none_or(|x| x.amount > 0.0)));
                                            }
                                            false
                                        })
                                        .with_hover(|ui, name, data| {
                                            if let Some(fluid) = data.fluids.get(name) {
                                                ui.add(PrototypeHover::new(data, fluid));
                                            }
                                        }),
                                ),
                        )
                        .changed()
                    {
                        changed = true;
                        instance.temperature = generator.maximum_temperature as i32;
                    }
                });
            }
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

    fn auto_populate(
        &mut self,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) {
        for generator in data.generators.values() {
            if !proj.is_prototype_accessible("entity", &generator.base.base.name) {
                continue;
            }
            if let Some(filter) = &generator.fluid_box.filter {
                if let Some(fluid) = data.fluids.get(filter)
                    && proj.is_prototype_accessible("entity", &generator.base.base.name)
                    && ((generator.burns_fluid && fluid.fuel_value.is_some())
                        || (!generator.burns_fluid
                            && fluid.heat_capacity.is_none_or(|x| x.amount > 0.0)))
                {
                    // for quality in 0..=proj.max_quality_level {
                    for temperature in data.temperatures.get(filter).expect("未初始化流体温度数据")
                    {
                        self.instances.push(GeneratorInstance {
                            generator: (generator.base.base.name.clone(), factory.major_quality)
                                .into(),
                            fluid: filter.clone(),
                            temperature: *temperature,
                        });
                    }
                    // }
                }
            } else {
                // 如果发电机没有指定输入流体，则尝试用所有可用
                for (fluid_name, fluid) in &data.fluids {
                    if proj.is_prototype_accessible("entity", &generator.base.base.name)
                        && ((generator.burns_fluid && fluid.fuel_value.is_some())
                            || (!generator.burns_fluid
                                && fluid.heat_capacity.is_none_or(|x| x.amount > 0.0)))
                    {
                        // for quality in 0..=proj.max_quality_level {
                        for temperature in data
                            .temperatures
                            .get(fluid_name)
                            .expect("未初始化流体温度数据")
                        {
                            self.instances.push(GeneratorInstance {
                                generator: (
                                    generator.base.base.name.clone(),
                                    factory.major_quality,
                                )
                                    .into(),
                                fluid: fluid_name.clone(),
                                temperature: *temperature,
                            });
                        }
                        // }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct BoilerMechanic {
    #[serde(skip)]
    pub operations: Vec<(usize, EntryOpRequest)>,

    pub instances: Vec<BoilerInstance>,
}

impl SolveContext for BoilerMechanic {
    type Game = DataContext;
    type Item = GenericItem;
}

#[typetag::serde(name = "factorio:boiler")]
impl FactorioMechanic for BoilerMechanic {
    fn name(&self) -> String {
        "锅炉".to_string()
    }

    fn editor_view(
        &mut self,
        ui: &mut egui::Ui,
        _data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        if ui.button("添加锅炉").clicked() {
            let new_config = BoilerInstance {
                boiler: "entity-unknown".into(),

                fluid: "fluid-unknown".to_string(),
                temperature: 25,
                fuel: None,
            };
            self.instances.push(new_config);
            changed = true;
        }
        changed
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

    fn instance_view(
        &mut self,
        idx: usize,
        ui: &mut egui::Ui,
        data: &DataContext,
        proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        let instance = &mut self.instances[idx];
        ui.vertical(|ui| {
            ui.label("锅炉");
            let entity_button = ui
                .add_sized(
                    [35.0, 35.0],
                    Icon::new(data, "entity", &instance.boiler.0).with_quality(instance.boiler.1),
                )
                .interact(egui::Sense::click());
            if ui
                .add(
                    SelectorModal::new(entity_button.id, data, "选择锅炉")
                        .with_toggle(entity_button.clicked())
                        .with_selector(
                            Selector::new(data, "entity")
                                .with_current(&mut instance.boiler)
                                .with_filter(|s: &IdWithQuality, f| {
                                    f.boilers.contains_key(&s.0)
                                        && proj.is_prototype_accessible("entity", &s.0)
                                }),
                        ),
                )
                .changed()
            {
                changed = true;
                if let Some(boiler) = data.boilers.get(&instance.boiler.0)
                    && let Some(filter) = &boiler.fluid_box.filter
                {
                    instance.fluid = filter.clone();
                    instance.temperature = data
                        .fluids
                        .get(&instance.fluid)
                        .unwrap_or_else(|| panic!("锅炉输入的流体 {} 不存在", &instance.fluid))
                        .default_temperature as i32;
                }
            }
        });
        ui.separator();
        if let Some(boiler) = data.boilers.get(&instance.boiler.0) {
            if let Some(filter) = &boiler.fluid_box.filter {
                // 如果锅炉指定了输入流体，则显示这个流体
                ui.vertical(|ui| {
                    ui.label("固定输入");
                    ui.add_sized([35.0, 35.0], Icon::new(data, "fluid", filter));
                });
            } else {
                // 如果锅炉没有指定输入流体，则允许用户选择输入流体
                ui.vertical(|ui| {
                    ui.label("编辑输入");
                    let fluid_button = ui
                        .add_sized([35.0, 35.0], Icon::new(data, "fluid", &instance.fluid))
                        .interact(egui::Sense::click());
                    if ui
                        .add(
                            SelectorModal::new(fluid_button.id, data, "选择输入流体")
                                .with_toggle(fluid_button.clicked())
                                .with_selector(
                                    Selector::new(data, "fluid")
                                        .with_current(&mut instance.fluid)
                                        .with_filter(|s: &str, f| {
                                            if let Some(fluid_prototype) = f.fluids.get(s) {
                                                return proj.is_prototype_accessible("fluid", s)
                                                    && fluid_prototype
                                                        .heat_capacity
                                                        .is_none_or(|x| x.amount > 0.0);
                                            }
                                            false
                                        })
                                        .with_hover(|ui, name, data| {
                                            if let Some(fluid) = data.fluids.get(name) {
                                                ui.add(PrototypeHover::new(data, fluid));
                                            }
                                        }),
                                ),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                });
            }
        }
        changed
    }

    fn submit_operations(&mut self) -> Vec<EntryOpResult> {
        self.instances.update_elements(&mut self.operations)
    }

    fn auto_populate(
        &mut self,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) {
        for boiler in data.boilers.values() {
            if !proj.is_prototype_accessible("entity", &boiler.base.base.name) {
                continue;
            }
            if let Some(filter) = &boiler.fluid_box.filter {
                if let Some(fluid) = data.fluids.get(filter)
                    && proj.is_prototype_accessible("entity", &boiler.base.base.name)
                    && fluid.heat_capacity.is_none_or(|x| x.amount > 0.0)
                {
                    // for quality in 0..=proj.max_quality_level {
                    for temperature in data.temperatures.get(filter).expect("未初始化流体温度数据")
                    {
                        self.instances.push(BoilerInstance {
                            boiler: (boiler.base.base.name.clone(), factory.major_quality).into(),
                            fluid: filter.clone(),
                            temperature: *temperature,
                            fuel: None,
                        });
                    }
                    // }
                }
            } else {
                // 如果锅炉没有指定输入流体，则尝试用所有可用的流体
                for (fluid_name, fluid) in &data.fluids {
                    if proj.is_prototype_accessible("entity", &boiler.base.base.name)
                        && fluid.heat_capacity.is_none_or(|x| x.amount > 0.0)
                    {
                        // for quality in 0..=proj.max_quality_level {
                        for temperature in data
                            .temperatures
                            .get(fluid_name)
                            .expect("未初始化流体温度数据")
                        {
                            self.instances.push(BoilerInstance {
                                boiler: (boiler.base.base.name.clone(), factory.major_quality)
                                    .into(),
                                fluid: fluid_name.clone(),
                                temperature: *temperature,
                                fuel: None,
                            });
                        }
                        // }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BoilerInstance {
    pub boiler: IdWithQuality,

    pub fluid: String,

    pub temperature: i32,

    pub fuel: Option<(String, i32)>,
}

impl SolveContext for BoilerInstance {
    type Game = DataContext;
    type Item = GenericItem;
}

impl AsFlow for BoilerInstance {
    fn as_flow(
        &self,
        data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> Flow<GenericItem> {
        let mut flow = Flow::new();
        if let Some(boiler) = data.boilers.get(&self.boiler.0) {
            let fluid = boiler.fluid_box.filter.as_ref().unwrap_or(&self.fluid);

            flow = flow_add(
                &flow,
                &boiler.get_flow(data, fluid, self.temperature as f64, &self.fuel),
                1.0,
            );
        }
        let idx = (self.boiler.1 as usize).min(data.qualities.len() - 1);
        let multiplier = data.qualities[idx].default_multiplier();
        flow.iter_mut().for_each(|v| *v.1 *= multiplier);
        flow
    }

    fn cost(&self, _data: &DataContext, _proj: &ProjectContext, _factory: &FactoryContext) -> f64 {
        if let Some(boiler) = _data.boilers.get(&self.boiler.0) {
            boiler
                .base
                .collision_box
                .as_ref()
                .map_or(16.0, |b| b.get_area())
        } else {
            16.0
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct FluidFuelMechanic {
    #[serde(skip)]
    pub operations: Vec<(usize, EntryOpRequest)>,

    pub instances: Vec<FluidFuelInstance>,
}

impl SolveContext for FluidFuelMechanic {
    type Game = DataContext;
    type Item = GenericItem;
}

#[typetag::serde(name = "factorio:fluid-fuel")]
impl FactorioMechanic for FluidFuelMechanic {
    fn name(&self) -> String {
        "流体燃烧".to_string()
    }

    fn editor_view(
        &mut self,
        ui: &mut egui::Ui,
        _data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        if ui.button("添加流体燃烧").clicked() {
            let new_config = FluidFuelInstance {
                fluid: "fluid-unknown".to_string(),
                temperature: 25,
            };
            self.instances.push(new_config);
            changed = true;
        }
        changed
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

    fn instance_view(
        &mut self,
        idx: usize,
        ui: &mut egui::Ui,
        data: &DataContext,
        proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        let instance = &mut self.instances[idx];
        ui.vertical(|ui| {
            ui.label("流体燃烧");
            let fluid_button = ui
                .add_sized([35.0, 35.0], Icon::new(data, "fluid", &instance.fluid))
                .interact(egui::Sense::click());
            if ui
                .add(
                    SelectorModal::new(fluid_button.id, data, "选择流体")
                        .with_toggle(fluid_button.clicked())
                        .with_selector(
                            Selector::new(data, "fluid")
                                .with_current(&mut instance.fluid)
                                .with_filter(|s: &str, f| {
                                    if let Some(fluid_prototype) = f.fluids.get(s) {
                                        return proj.is_prototype_accessible("fluid", s)
                                            && fluid_prototype
                                                .fuel_value
                                                .is_some_and(|x| x.amount > 0.0);
                                    }
                                    false
                                })
                                .with_hover(|ui, name, data| {
                                    if let Some(fluid) = data.fluids.get(name) {
                                        ui.add(PrototypeHover::new(data, fluid));
                                    }
                                }),
                        ),
                )
                .changed()
            {
                changed = true;
                instance.temperature = data
                    .fluids
                    .get(&instance.fluid)
                    .unwrap_or_else(|| panic!("流体燃烧的流体 {} 不存在", &instance.fluid))
                    .default_temperature as i32;
            }
        });
        ui.separator();
        changed
    }

    fn submit_operations(&mut self) -> Vec<EntryOpResult> {
        self.instances.update_elements(&mut self.operations)
    }

    fn auto_populate(
        &mut self,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) {
        for (fluid, temperatures) in &data.temperatures {
            if let Some(fluid_prototype) = data.fluids.get(fluid)
                && fluid_prototype.fuel_value.is_some_and(|x| x.amount > 0.0)
            {
                for temperature in temperatures {
                    self.instances.push(FluidFuelInstance {
                        fluid: fluid.clone(),
                        temperature: *temperature,
                    });
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FluidFuelInstance {
    pub fluid: String,

    pub temperature: i32,
}

impl SolveContext for FluidFuelInstance {
    type Game = DataContext;
    type Item = GenericItem;
}

impl AsFlow for FluidFuelInstance {
    fn as_flow(
        &self,
        data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> Flow<GenericItem> {
        let mut flow = Flow::new();
        if let Some(fluid) = data.fluids.get(&self.fluid) {
            if let Some(fuel_value) = fluid.fuel_value {
                index_map_update_entry(
                    &mut flow,
                    GenericItem::Fluid {
                        name: self.fluid.clone(),
                        temperature: [self.temperature, self.temperature],
                    },
                    -1.0,
                );
                index_map_update_entry(
                    &mut flow,
                    GenericItem::FluidFuel {
                        filter: self.fluid.clone().into(),
                    },
                    fuel_value.amount * 60.0,
                );
            }
        }
        flow
    }

    fn cost(&self, _data: &DataContext, _proj: &ProjectContext, _factory: &FactoryContext) -> f64 {
        // 1.0 / 10240.0 // 几乎无成本
        0.0
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct FluidHeatMechanic {
    #[serde(skip)]
    pub operations: Vec<(usize, EntryOpRequest)>,

    pub instances: Vec<FluidHeatInstance>,
}

impl SolveContext for FluidHeatMechanic {
    type Game = DataContext;
    type Item = GenericItem;
}

#[typetag::serde(name = "factorio:fluid-heat")]
impl FactorioMechanic for FluidHeatMechanic {
    fn name(&self) -> String {
        "流体供热".to_string()
    }

    fn editor_view(
        &mut self,
        ui: &mut egui::Ui,
        _data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        if ui.button("添加流体供热").clicked() {
            let new_config = FluidHeatInstance {
                fluid: "fluid-unknown".to_string(),
                temperature: 25,
            };
            self.instances.push(new_config);
            changed = true;
        }
        changed
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

    fn instance_view(
        &mut self,
        idx: usize,
        ui: &mut egui::Ui,
        data: &DataContext,
        proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        let instance = &mut self.instances[idx];
        ui.vertical(|ui| {
            ui.label("流体供热");
            let fluid_button = ui
                .add_sized([35.0, 35.0], Icon::new(data, "fluid", &instance.fluid))
                .interact(egui::Sense::click());
            if ui
                .add(
                    SelectorModal::new(fluid_button.id, data, "选择流体")
                        .with_toggle(fluid_button.clicked())
                        .with_selector(
                            Selector::new(data, "fluid")
                                .with_current(&mut instance.fluid)
                                .with_filter(|s: &str, f| {
                                    if let Some(fluid_prototype) = f.fluids.get(s) {
                                        return proj.is_prototype_accessible("fluid", s)
                                            && fluid_prototype
                                                .heat_capacity
                                                .is_none_or(|x| x.amount > 0.0);
                                    }
                                    false
                                })
                                .with_hover(|ui, name, data| {
                                    if let Some(fluid) = data.fluids.get(name) {
                                        ui.add(PrototypeHover::new(data, fluid));
                                    }
                                }),
                        ),
                )
                .changed()
            {
                changed = true;
                instance.temperature = data
                    .fluids
                    .get(&instance.fluid)
                    .unwrap_or_else(|| panic!("流体燃烧的流体 {} 不存在", &instance.fluid))
                    .default_temperature as i32;
            }
        });
        ui.separator();
        changed
    }

    fn submit_operations(&mut self) -> Vec<EntryOpResult> {
        self.instances.update_elements(&mut self.operations)
    }

    fn auto_populate(
        &mut self,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) {
        for (fluid, temperatures) in &data.temperatures {
            if let Some(fluid_prototype) = data.fluids.get(fluid)
                && fluid_prototype.heat_capacity.is_none_or(|x| x.amount > 0.0)
            {
                for temperature in temperatures {
                    self.instances.push(FluidHeatInstance {
                        fluid: fluid.clone(),
                        temperature: *temperature,
                    });
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FluidHeatInstance {
    pub fluid: String,

    pub temperature: i32,
}

impl SolveContext for FluidHeatInstance {
    type Game = DataContext;
    type Item = GenericItem;
}

impl AsFlow for FluidHeatInstance {
    fn as_flow(
        &self,
        data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> Flow<GenericItem> {
        let mut flow = Flow::new();
        if let Some(fluid) = data.fluids.get(&self.fluid) {
            let heat_capacity = fluid
                .heat_capacity
                .unwrap_or(EnergyAmount { amount: 1000.0 });
            index_map_update_entry(
                &mut flow,
                GenericItem::Fluid {
                    name: self.fluid.clone(),
                    temperature: [self.temperature, self.temperature],
                },
                -1.0,
            );
            index_map_update_entry(
                &mut flow,
                GenericItem::FluidHeat {
                    filter: self.fluid.clone().into(),
                },
                heat_capacity.amount
                    * 60.0
                    * (self.temperature - fluid.default_temperature as i32) as f64,
            );
        }
        flow
    }

    fn cost(&self, _data: &DataContext, _proj: &ProjectContext, _factory: &FactoryContext) -> f64 {
        // 1.0 / 10240.0 // 几乎无成本
        0.0
    }
}
