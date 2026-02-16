use crate::factorio::{EntityPrototype, common::*};

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
                    && actual_power_output > max_power_output {
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
