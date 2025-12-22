use serde::Deserialize;

use crate::ctx::factorio::common::{EnergyAmount, PrototypeBase, HasPrototypeBase};

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct FluidPrototype {
    #[serde(flatten)]
    pub(crate) base: PrototypeBase,

    pub(crate) default_temperature: f64,

    pub(crate) max_temperature: Option<f64>,

    /// 一单位液体上升一摄氏度所需的能量
    pub(crate) heat_capacity: Option<EnergyAmount>,

    /// 燃烧每单位液体所释放的能量
    pub(crate) fuel_value: Option<EnergyAmount>,
}

impl HasPrototypeBase for FluidPrototype {
    fn base(&self) -> &PrototypeBase {
        &self.base
    }
}