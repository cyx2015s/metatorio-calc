use serde::Deserialize;

use crate::factorio::common::{EnergyAmount, PrototypeBase, HasPrototypeBase};

#[derive(Debug, Clone, Deserialize)]
pub struct FluidPrototype {
    #[serde(flatten)]
    pub base: PrototypeBase,

    pub default_temperature: f64,

    pub max_temperature: Option<f64>,

    /// 一单位液体上升一摄氏度所需的能量
    pub heat_capacity: Option<EnergyAmount>,

    /// 燃烧每单位液体所释放的能量
    pub fuel_value: Option<EnergyAmount>,
}

impl HasPrototypeBase for FluidPrototype {
    fn base(&self) -> &PrototypeBase {
        &self.base
    }
}