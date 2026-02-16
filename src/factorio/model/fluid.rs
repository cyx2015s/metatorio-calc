use crate::{
    concept::Flow,
    factorio::{DataContext, EntityPrototype, GenericItem, common::*, energy_source_as_flow},
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

impl HasPrototypeBase for GeneratorPrototype {
    fn base(&self) -> &PrototypeBase {
        &self.base.base
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

#[derive(Debug, Clone, Default, serde::Deserialize)]
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
        if fulfillment != 0.0 {
            log::warn!(
                "锅炉 {} 的能量需求没有完全满足，满足度为 {}",
                self.base.base.name,
                fulfillment
            );
            log::warn!("使用的燃料是 {:?}", fuel);
        }
        match self.mode {
            BoilerMode::HeatFluidInside => {
                // TODO 在锅炉内部将流体加热
                // 这个加热是连续的过程，需要用户指定加热到多少度，才能计算出流体的消耗量和产出量
                // 考虑到原版没有直接读取管道内流体温度的机制，实际上玩家也无法控制吧？
                // 暂时先不实现这个模式了，等以后有需要再说
                todo!();
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
