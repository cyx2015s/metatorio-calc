use crate::factorio::common::*;

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
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

pub fn calc_quality_distribution(
    qualities: &[QualityPrototype],
    quality_bonus: f64,
    base_quality: usize,
    maximum_quality: usize,
) -> Vec<f64> {
    let mut result = vec![0.0; qualities.len()];
    result[base_quality] = quality_bonus; // 有这么多能参与品质转移
    for idx in base_quality..(maximum_quality.clamp(base_quality, qualities.len() - 1)) {
        // idx，jdx，令人忍俊不禁
        let jdx = idx + 1;
        result[jdx] = result[idx] * qualities[idx].next_probability;
    }
    for idx in (base_quality + 1)..result.len() {
        let hdx = idx - 1;
        result[hdx] -= result[idx];
    }
    result[base_quality] += 1.0 - quality_bonus; // 剩下的都是基础品质
    for idx in 0..(result.len() - 1) {
        if result[idx] < 0.0 {
            result[idx + 1] += result[idx];
            result[idx] = 0.0;
        }
    }
    result
}

#[test]
fn test_calc_quality_distribution() {
    use crate::factorio::model::context::FactorioContext;
    let data = include_str!("../../../assets/data-raw-dump.json");
    let value = serde_json::from_str(&data).unwrap();
    let ctx = FactorioContext::load(&value);

    dbg!(calc_quality_distribution(&ctx.qualities, 1.0, 0, 4));
    dbg!(calc_quality_distribution(&ctx.qualities, 10.0, 0, 4));
    dbg!(calc_quality_distribution(&ctx.qualities, 100.0, 0, 4));
    dbg!(calc_quality_distribution(&ctx.qualities, 200.0, 0, 4));
}
