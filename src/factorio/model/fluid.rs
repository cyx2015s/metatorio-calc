use crate::{
    concept::{Flow, SolveContext},
    factorio::{
        DataContext, DualVar, EntityPrototype, ProjectContext, common::*, energy_source_as_flow,
        icon::Icon, modal::SelectorModal, planner::FactoryContext, selector::Selector,
        surface_condition_satisfied,
    },
    math::flow_add,
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
    /// 输入指定温度的流体时，流体的消耗量和电量产出
    /// 返回：(每秒消耗的流体量, 每秒产生的电量)
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
    ) -> Flow<DualVar> {
        let mut flow = Flow::default();
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
                if target_fluid_temperature - temperature == 0.0 {
                    return flow;
                }
                let amount = self.energy_consumption.amount * 60.0 // 功率
                    / source_heat_capacity.amount // 输入流体的比热容
                    / (target_fluid_temperature - temperature); // 温度差
                index_map_update_entry(
                    &mut flow,
                    DualVar::Fluid {
                        name: fluid.clone(),
                        temperature: [temperature as i32, temperature as i32],
                    },
                    -amount,
                );
                index_map_update_entry(
                    &mut flow,
                    DualVar::Fluid {
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
    #[serde(flatten)]
    pub instances: ReactVec<GeneratorInstance>,

    #[serde(skip)]
    pub show_suggestion: bool,
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
    type Item = DualVar;
}

impl AsFlow for GeneratorInstance {
    fn as_flow(
        &self,
        data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> Flow<DualVar> {
        let mut flow = Flow::default();
        if let Some(generator) = data.generators.get(&self.generator.0) {
            if let Some(filter) = generator.fluid_box.filter.as_ref() {
                let fluid = data.fluids.get(filter).expect("发电机输入的流体不存在");
                let (fluid_usage, power_output) =
                    generator.get_output(fluid, self.temperature as f64);
                index_map_update_entry(
                    &mut flow,
                    DualVar::Fluid {
                        name: filter.clone(),
                        temperature: [self.temperature, self.temperature],
                    },
                    -fluid_usage,
                );
                index_map_update_entry(&mut flow, DualVar::Electricity, power_output);
            } else if let Some(fluid) = data.fluids.get(&self.fluid) {
                let (fluid_usage, power_output) =
                    generator.get_output(fluid, self.temperature as f64);
                index_map_update_entry(
                    &mut flow,
                    DualVar::Fluid {
                        name: self.fluid.clone(),
                        temperature: [self.temperature, self.temperature],
                    },
                    -fluid_usage,
                );
                index_map_update_entry(&mut flow, DualVar::Electricity, power_output);
            }
            generator
                .energy_source
                .emissions_per_minute
                .as_ref()
                .inspect(|map| {
                    for (pollutant, amount) in map.iter() {
                        index_map_update_entry(
                            &mut flow,
                            DualVar::Pollution {
                                name: pollutant.clone(),
                            },
                            amount / 60.0,
                        );
                    }
                });

            let idx = (self.generator.1 as usize).min(data.qualities.len() - 1);
            let multiplier = data.qualities[idx].default_multiplier();
            flow.iter_mut().for_each(|v| *v.1 *= multiplier);
        }

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
    type Item = DualVar;
}

#[typetag::serde(name = "factorio:generator")]
impl SerdeFactorioMechanic for GeneratorMechanic {}

impl FactorioMechanic for GeneratorMechanic {
    fn name(&self) -> String {
        t!("metatorio.fluid-generator").to_string()
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
        if ui
            .button(t!("metatorio.add-fluid-generator").to_string().as_str())
            .clicked()
        {
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
            ui.label(t!("metatorio.generator").to_string());
            let entity_button = ui.add_sized(
                [35.0, 35.0],
                Icon::new(data, "entity", &instance.generator.0).with_quality(instance.generator.1),
            );
            if ui
                .add(
                    SelectorModal::new(entity_button.id, "选择发电机")
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
                    ui.label(t!("metatorio.fixed-input").to_string());
                    ui.add_sized([35.0, 35.0], Icon::new(data, "fluid", filter));
                });
            } else {
                // 如果发电机没有指定输入流体，则允许用户选择输入流体
                ui.vertical(|ui| {
                    ui.label(t!("metatorio.edit-input").to_string());
                    let fluid_button =
                        ui.add_sized([35.0, 35.0], Icon::new(data, "fluid", &instance.fluid));
                    if ui
                        .add(
                            SelectorModal::new(fluid_button.id, "选择输入流体")
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
            if let Some(fluid) = data.fluids.get(&instance.fluid) {
                ui.separator();
                temperature_editor(ui, data, &mut changed, &mut instance.temperature, fluid);
            }
        }
        changed
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
            if let Some(surface_properties) = factory.get_current_surface_properties(data)
                && !surface_condition_satisfied(
                    &generator.base.surface_conditions,
                    surface_properties,
                    &data.surface_properties,
                )
            {
                continue;
            }
            if let Some(filter) = &generator.fluid_box.filter {
                if let Some(fluid) = data.fluids.get(filter)
                    && proj.is_prototype_accessible("entity", &generator.base.base.name)
                    && ((generator.burns_fluid && fluid.fuel_value.is_some())
                        || (!generator.burns_fluid
                            && fluid.heat_capacity.is_none_or(|x| x.amount > 0.0)))
                {
                    for temperature in data.temperatures.get(filter).expect("未初始化流体温度数据")
                    {
                        self.instances.push(GeneratorInstance {
                            generator: (generator.base.base.name.clone(), factory.major_quality)
                                .into(),
                            fluid: filter.clone(),
                            temperature: *temperature,
                        });
                    }
                }
            } else {
                // 如果发电机没有指定输入流体，则尝试用所有可用
                for (fluid_name, fluid) in &data.fluids {
                    if proj.is_prototype_accessible("entity", &generator.base.base.name)
                        && ((generator.burns_fluid && fluid.fuel_value.is_some())
                            || (!generator.burns_fluid
                                && fluid.heat_capacity.is_none_or(|x| x.amount > 0.0)))
                    {
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
                    }
                }
            }
        }
    }

    fn update_suggestion(
        &mut self,
        _data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
        item: &DualVar,
        amount: f64,
    ) {
        self.show_suggestion = item == &DualVar::Electricity && amount > 0.0;
    }

    fn suggestion_view(
        &mut self,
        ui: &mut egui::Ui,
        data: &DataContext,
        proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        let mut output = None;
        changed |= ui
            .add(
                Selector::new(data, "entity")
                    .with_filter(|s: &IdWithQuality, f| {
                        f.generators.contains_key(&s.0)
                            && proj.is_prototype_accessible("entity", &s.0)
                    })
                    .with_output(&mut output),
            )
            .changed();
        if let Some(output) = output
            && let Some(generator) = data.generators.get(&output.0)
        {
            if let Some(filter) = &generator.fluid_box.filter {
                if let Some(fluid) = data.fluids.get(filter)
                    && proj.is_prototype_accessible("entity", &generator.base.base.name)
                    && ((generator.burns_fluid && fluid.fuel_value.is_some())
                        || (!generator.burns_fluid
                            && fluid.heat_capacity.is_none_or(|x| x.amount > 0.0)))
                {
                    self.instances.push(GeneratorInstance {
                        generator: output,
                        fluid: filter.clone(),
                        temperature: generator.maximum_temperature as i32,
                    });
                }
            } else {
                self.instances.push(GeneratorInstance {
                    generator: output,
                    fluid: "fluid-unknown".to_string(),
                    temperature: 25,
                });
            }
        }

        changed
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct BoilerMechanic {
    #[serde(flatten)]
    pub instances: ReactVec<BoilerInstance>,
}

impl SolveContext for BoilerMechanic {
    type Game = DataContext;
    type Item = DualVar;
}

#[typetag::serde(name = "factorio:boiler")]
impl SerdeFactorioMechanic for BoilerMechanic {}
impl FactorioMechanic for BoilerMechanic {
    fn name(&self) -> String {
        t!("metatorio.boiler").to_string()
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
        if ui
            .button(t!("metatorio.add-boiler").to_string().as_str())
            .clicked()
        {
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
            ui.label(t!("metatorio.boiler"));
            let entity_button = ui.add_sized(
                [35.0, 35.0],
                Icon::new(data, "entity", &instance.boiler.0).with_quality(instance.boiler.1),
            );
            if ui
                .add(
                    SelectorModal::new(
                        entity_button.id,
                        t!("metatorio.select-boiler").to_string().as_str(),
                    )
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
                    ui.label(t!("metatorio.fixed-input"));
                    ui.add_sized([35.0, 35.0], Icon::new(data, "fluid", filter));
                });
            } else {
                // 如果锅炉没有指定输入流体，则允许用户选择输入流体
                ui.vertical(|ui| {
                    ui.label(t!("metatorio.edit-input"));
                    let fluid_button =
                        ui.add_sized([35.0, 35.0], Icon::new(data, "fluid", &instance.fluid));
                    if ui
                        .add(
                            SelectorModal::new(
                                fluid_button.id,
                                t!("metatorio.select-fluid").to_string().as_str(),
                            )
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
                                    }),
                            ),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                });
            }

            // 添加温度选择
            if let Some(fluid) = data.fluids.get(&instance.fluid) {
                ui.separator();
                temperature_editor(ui, data, &mut changed, &mut instance.temperature, fluid);
            }
        }
        changed
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
            if let Some(surface_properties) = factory.get_current_surface_properties(data)
                && !surface_condition_satisfied(
                    &boiler.base.surface_conditions,
                    surface_properties,
                    &data.surface_properties,
                )
            {
                continue;
            }
            if let Some(filter) = &boiler.fluid_box.filter {
                if let Some(fluid) = data.fluids.get(filter)
                    && proj.is_prototype_accessible("entity", &boiler.base.base.name)
                    && fluid.heat_capacity.is_none_or(|x| x.amount > 0.0)
                {
                    for temperature in data.temperatures.get(filter).expect("未初始化流体温度数据")
                    {
                        self.instances.push(BoilerInstance {
                            boiler: (boiler.base.base.name.clone(), factory.major_quality).into(),
                            fluid: filter.clone(),
                            temperature: *temperature,
                            fuel: None,
                        });
                    }
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
                    }
                }
            }
        }
    }
}

fn temperature_editor(
    ui: &mut egui::Ui,
    data: &DataContext,
    changed: &mut bool,
    editing_temperature: &mut i32,
    fluid: &FluidPrototype,
) {
    ui.vertical(|ui| {
        let default_temp = fluid.default_temperature as i32;
        let max_temp = fluid
            .max_temperature
            .map(|v| v as i32)
            .unwrap_or(default_temp);
        // 从data.temperatures中选择固定温度
        if let Some(temperatures) = data.temperatures.get(&fluid.base.name) {
            ui.horizontal(|ui| {
                ui.menu_button(t!("metatorio.select-preset-temperature"), |ui| {
                    for &temp in temperatures {
                        if temp >= default_temp
                            && temp <= max_temp
                            && ui
                                .selectable_label(
                                    *editing_temperature == temp,
                                    format!("{}℃", temp),
                                )
                                .clicked()
                        {
                            *editing_temperature = temp;
                            *changed = true;
                        }
                    }
                });
            });
        }

        // 允许手动输入温度
        ui.horizontal(|ui| {
            ui.label(t!("metatorio.edit-temperature").to_string());
            if ui
                .add(
                    egui::DragValue::new(editing_temperature)
                        .speed(1)
                        .range(default_temp..=max_temp)
                        .suffix("℃")
                        .clamp_existing_to_range(true),
                )
                .changed()
            {
                *changed = true;
            }
        });
    });
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
    type Item = DualVar;
}

impl AsFlow for BoilerInstance {
    fn as_flow(
        &self,
        data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> Flow<DualVar> {
        let mut flow = Flow::default();
        if let Some(boiler) = data.boilers.get(&self.boiler.0) {
            let fluid = boiler.fluid_box.filter.as_ref().unwrap_or(&self.fluid);

            flow = flow_add(
                &flow,
                &boiler.get_flow(data, fluid, self.temperature as f64, &self.fuel),
                1.0,
            );

            boiler
                .energy_source
                .emissions_per_minute()
                .as_ref()
                .inspect(|map| {
                    for (pollutant, amount) in map.iter() {
                        index_map_update_entry(
                            &mut flow,
                            DualVar::Pollution {
                                name: pollutant.clone(),
                            },
                            amount / 60.0,
                        );
                    }
                });

            let idx = (self.boiler.1 as usize).min(data.qualities.len() - 1);
            let multiplier = data.qualities[idx].default_multiplier();
            flow.iter_mut().for_each(|v| *v.1 *= multiplier);
        }
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
    #[serde(flatten)]
    pub instances: ReactVec<FluidFuelInstance>,
}

impl SolveContext for FluidFuelMechanic {
    type Game = DataContext;
    type Item = DualVar;
}

#[typetag::serde(name = "factorio:fluid-fuel")]
impl SerdeFactorioMechanic for FluidFuelMechanic {}

impl FactorioMechanic for FluidFuelMechanic {
    fn name(&self) -> String {
        t!("metatorio.fluid-fuel").to_string()
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
        if ui.button(t!("metatorio.add-fluid-fuel")).clicked() {
            let new_config = FluidFuelInstance {
                fluid: "fluid-unknown".to_string(),
                temperature: 25,
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
        proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        let instance = &mut self.instances[idx];
        ui.vertical(|ui| {
            ui.label(t!("metatorio.fluid-fuel").to_string());
            let fluid_button =
                ui.add_sized([35.0, 35.0], Icon::new(data, "fluid", &instance.fluid));
            if ui
                .add(
                    SelectorModal::new(
                        fluid_button.id,
                        t!("metatorio.select-fluid").to_string().as_str(),
                    )
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
        if let Some(fluid) = data.fluids.get(&instance.fluid) {
            ui.separator();
            temperature_editor(ui, data, &mut changed, &mut instance.temperature, fluid);
        }
        changed
    }

    fn auto_populate(
        &mut self,
        data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
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
    type Item = DualVar;
}

impl AsFlow for FluidFuelInstance {
    fn as_flow(
        &self,
        data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> Flow<DualVar> {
        let mut flow = Flow::default();
        if let Some(fluid) = data.fluids.get(&self.fluid)
            && let Some(fuel_value) = fluid.fuel_value
        {
            index_map_update_entry(
                &mut flow,
                DualVar::Fluid {
                    name: self.fluid.clone(),
                    temperature: [self.temperature, self.temperature],
                },
                -1.0,
            );
            index_map_update_entry(
                &mut flow,
                DualVar::FluidFuel {
                    filter: self.fluid.clone().into(),
                },
                fuel_value.amount * 60.0,
            );
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
    #[serde(flatten)]
    pub instances: ReactVec<FluidHeatInstance>,
}

impl SolveContext for FluidHeatMechanic {
    type Game = DataContext;
    type Item = DualVar;
}

#[typetag::serde(name = "factorio:fluid-heat")]
impl SerdeFactorioMechanic for FluidHeatMechanic {}
impl FactorioMechanic for FluidHeatMechanic {
    fn name(&self) -> String {
        t!("metatorio.fluid-heat").to_string()
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
        if ui.button(t!("metatorio.add-fluid-heat")).clicked() {
            let new_config = FluidHeatInstance {
                fluid: "fluid-unknown".to_string(),
                temperature: 25,
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
        proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        let instance = &mut self.instances[idx];
        ui.vertical(|ui| {
            ui.label(t!("metatorio.fluid-heat"));
            let fluid_button =
                ui.add_sized([35.0, 35.0], Icon::new(data, "fluid", &instance.fluid));
            if ui
                .add(
                    SelectorModal::new(
                        fluid_button.id,
                        t!("metatorio.select-fluid").to_string().as_str(),
                    )
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

        if let Some(fluid) = data.fluids.get(&instance.fluid) {
            ui.separator();
            temperature_editor(ui, data, &mut changed, &mut instance.temperature, fluid);
        }
        changed
    }

    fn auto_populate(
        &mut self,
        data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
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
    type Item = DualVar;
}

impl AsFlow for FluidHeatInstance {
    fn as_flow(
        &self,
        data: &DataContext,
        _proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> Flow<DualVar> {
        let mut flow = Flow::default();
        if let Some(fluid) = data.fluids.get(&self.fluid) {
            if self.temperature <= fluid.default_temperature as i32 {
                // 如果温度不高于默认温度，则不产生热量，也不消耗液体
                return flow;
            }
            let heat_capacity = fluid
                .heat_capacity
                .unwrap_or(EnergyAmount { amount: 1000.0 });
            index_map_update_entry(
                &mut flow,
                DualVar::Fluid {
                    name: self.fluid.clone(),
                    temperature: [self.temperature, self.temperature],
                },
                -1.0,
            );
            index_map_update_entry(
                &mut flow,
                DualVar::FluidHeat {
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
