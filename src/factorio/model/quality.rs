use serde::Deserialize;

use crate::factorio::common::{Color, HasPrototypeBase, PrototypeBase};

#[derive(Debug, Clone, Deserialize)]
pub struct QualityPrototype {
    #[serde(flatten)]
    pub base: PrototypeBase,

    /// 品质链条基本是线性的，这个用于在上下文中获取下标
    #[serde(default)]
    pub index: usize,

    pub level: f64,
    pub color: Color,

    pub next: Option<String>,

    #[serde(default)]
    pub next_probability: f64, // 0
    #[serde(default)]
    beacon_power_usage_multiplier: Option<f64>, // 1
    #[serde(default)]
    mining_drill_resource_drain_multiplier: Option<f64>, // 1
    #[serde(default)]
    science_pack_drain_multiplier: Option<f64>, // 1
    #[serde(default)]
    default_multiplier: Option<f64>, // 1 + 0.3 * level
    #[serde(default)]
    inserter_speed_multiplier: Option<f64>, // default_multiplier
    #[serde(default)]
    fluid_wagon_capacity_multiplier: Option<f64>, // default_multiplier
    #[serde(default)]
    inventory_size_multiplier: Option<f64>, // default_multiplier
    #[serde(default)]
    lab_research_speed_multiplier: Option<f64>, // default_multiplier
    #[serde(default)]
    crafting_machine_speed_multiplier: Option<f64>, // default_multiplier
    #[serde(default)]
    crafting_machine_energy_usage_multiplier: Option<f64>, // 1
    #[serde(default)]
    tool_durability_multiplier: Option<f64>, // 1 + level
    #[serde(default)]
    accumulator_capacity_multiplier: Option<f64>, // 1 + level
    #[serde(default)]
    beacon_module_slots_bonus: Option<f64>, // level
    #[serde(default)]
    crafting_machine_module_slots_bonus: Option<f64>, // level
    #[serde(default)]
    mining_drill_module_slots_bonus: Option<f64>, // level
    #[serde(default)]
    lab_module_slots_bonus: Option<f64>, // level
}

impl HasPrototypeBase for QualityPrototype {
    fn base(&self) -> &PrototypeBase {
        &self.base
    }
}
