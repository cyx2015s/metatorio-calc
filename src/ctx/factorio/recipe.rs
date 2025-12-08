use std::fmt::Debug;

use serde::Deserialize;

use crate::ctx::factorio::common::{
    Dict, EffectReceiver, EffectTypeLimitation, EnergyAmount, EnergySource, PrototypeBase,
    as_vec_or_empty, option_as_vec_or_empty,
};

const RECIPE_TYPE: &str = "recipe";

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct RecipePrototype {
    #[serde(flatten)]
    pub(crate) base: PrototypeBase,

    category: Option<String>,
    #[serde(deserialize_with = "as_vec_or_empty")]
    additional_categories: Vec<String>,

    #[serde(deserialize_with = "as_vec_or_empty")]
    ingredients: Vec<RecipeIngredient>,

    #[serde(deserialize_with = "as_vec_or_empty")]
    results: Vec<RecipeResult>,

    #[serde(deserialize_with = "option_as_vec_or_empty")]
    allowed_module_categories: Option<Vec<String>>,

    /// 制作时间（秒）
    pub(crate) energy_required: f64,

    /// 配方污染倍数
    emissions_multiplier: f64,

    /// 最大产能加成
    maximum_productivity: f64,

    /// 开局是否可用
    enabled: bool,

    /// 产物若为可变质，是否永远新鲜
    result_is_always_fresh: bool,

    /// 是否允许使用降低能耗的插件
    allow_consumption: bool,

    /// 是否允许使用增加速度的插件
    allow_speed: bool,

    /// 是否允许使用增加产能的插件
    allow_productivity: bool,

    /// 是否允许使用降低污染的插件
    allow_pollution: bool,

    /// 是否允许使用增加品质的插件
    allow_quality: bool,
}

impl Default for RecipePrototype {
    fn default() -> Self {
        RecipePrototype {
            base: PrototypeBase {
                r#type: "recipe".to_string(),
                name: "recipe-unknown".to_string(),
                order: String::new(),
                subgroup: String::new(),
                hidden: false,
                parameter: false,
            },
            category: None,
            additional_categories: Vec::new(),
            ingredients: Vec::new(),
            results: Vec::new(),
            allowed_module_categories: None,
            energy_required: 0.5,
            emissions_multiplier: 1.0,
            maximum_productivity: 3.0,
            enabled: true,
            result_is_always_fresh: false,
            allow_consumption: true,
            allow_speed: true,
            allow_productivity: false,
            allow_pollution: true,
            allow_quality: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum RecipeIngredient {
    /// 物品原料
    #[serde(rename = "item")]
    Item(ItemIngredient),
    /// 流体原料
    #[serde(rename = "fluid")]
    Fluid(FluidIngredient),
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ItemIngredient {
    pub(crate) name: String,
    pub(crate) amount: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct FluidIngredient {
    pub(crate) name: String,
    pub(crate) amount: f64,
    pub(crate) temperature: Option<f64>,
    pub(crate) min_temperature: Option<f64>,
    pub(crate) max_temperature: Option<f64>,
    pub(crate) fluidbox_index: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum RecipeResult {
    /// 物品产物
    #[serde(rename = "item")]
    Item(ItemResult),
    /// 流体产物
    #[serde(rename = "fluid")]
    Fluid(FluidResult),
}

#[derive(Clone, Deserialize)]
pub(crate) struct ItemResult {
    pub(crate) name: String,
    pub(crate) amount: Option<f64>,
    pub(crate) amount_min: Option<f64>,
    pub(crate) amount_max: Option<f64>,
    pub(crate) probability: f64,
    pub(crate) ignored_by_stats: Option<f64>,
    pub(crate) ignored_by_productivity: Option<f64>,
    pub(crate) extra_count_fraction: f64,
    pub(crate) percent_spoiled: f64,
}

impl Default for ItemResult {
    fn default() -> Self {
        ItemResult {
            name: String::new(),
            amount: None,
            amount_min: None,
            amount_max: None,
            probability: 1.0,
            ignored_by_stats: None,
            ignored_by_productivity: None,
            extra_count_fraction: 0.0,
            percent_spoiled: 0.0,
        }
    }
}

impl Debug for ItemResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (base_yield, extra_yield) = self.normalized_output();
        f.debug_struct("ItemResult")
            .field("name", &self.name)
            .field("<base yield>", &base_yield)
            .field("<productivity yield>", &extra_yield)
            .field("percent_spoiled", &self.percent_spoiled)
            .finish()
    }
}

impl ItemResult {
    /// 计算当前配方的实际单次产量和每次结算产能加成时的额外产量
    pub(crate) fn normalized_output(&self) -> (f64, f64) {
        let extra = self.extra_count_fraction;
        let prob = self.probability;
        let ignore = match self.ignored_by_productivity {
            Some(value) => value,
            None => self.ignored_by_stats.unwrap_or(0.0),
        };
        match self.amount {
            Some(amount) => {
                // 产出分别为：
                // amount (prob * (1 - extra))
                // amount + 1 (prob * extra)
                // 1 (1 - prob * extra)
                let base = amount as f64;
                let productivity = f64::max((base - ignore) * prob * (1.0 - extra), 0.0)
                    + f64::max((base + 1.0 - ignore) * prob * extra, 0.0)
                    + f64::max((1.0 - ignore) * (1.0 - prob) * extra, 0.0);
                (base * prob + extra, productivity)
            }
            None => {
                // 产出分别为：
                // min ~ max (prob * (1 - extra))
                // (min ~ max) + 1 (prob * extra)
                // 1 (1 - prob * extra)
                // 减去 ignore 前要先判断范围，还要求平均
                let min = match self.amount_min {
                    Some(value) => value as f64,
                    None => 0.0,
                };
                let max = match self.amount_max {
                    Some(value) => value as f64,
                    None => min,
                };
                let max = f64::max(max, min);

                let productivity = f64::max(
                    // 首项加末项乘项数除以状态数乘概率除以二
                    (max - ignore + f64::max(min - ignore, 0.0))
                        * (max - f64::max(min - ignore, 0.0) + 1.0)
                        / (max - min + 1.0)
                        / 2.0
                        * prob
                        * (1.0 - extra),
                    0.0,
                ) + f64::max(
                    (max + 1.0 - ignore + f64::max(min + 1.0 - ignore, 0.0))
                        * (max - f64::max(min + 1.0 - ignore, 0.0) + 1.0)
                        / (max - min + 1.0)
                        / 2.0
                        * prob
                        * extra,
                    0.0,
                ) + f64::max((extra - ignore) * (1.0 - prob) * extra, 0.0);
                (((max + min) / 2.0) * prob + extra, productivity)
            }
        }
    }
}

#[derive(Clone, Deserialize)]
pub(crate) struct FluidResult {
    pub(crate) name: String,
    pub(crate) amount: Option<f64>,
    pub(crate) amount_min: Option<f64>,
    pub(crate) amount_max: Option<f64>,
    pub(crate) probability: f64,
    pub(crate) ignored_by_stats: Option<f64>,
    pub(crate) ignored_by_productivity: Option<f64>,
    pub(crate) temperature: Option<f64>,
    pub(crate) min_temperature: Option<f64>,
    pub(crate) max_temperature: Option<f64>,
    pub(crate) fluidbox_index: f64,
}

impl Default for FluidResult {
    fn default() -> Self {
        FluidResult {
            name: String::new(),
            amount: None,
            amount_min: None,
            amount_max: None,
            probability: 1.0,
            ignored_by_stats: None,
            ignored_by_productivity: None,
            temperature: None,
            min_temperature: None,
            max_temperature: None,
            fluidbox_index: 0.0,
        }
    }
}

impl Debug for FluidResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (base_yield, extra_yield) = self.normalized_output();
        f.debug_struct("FluidResult")
            .field("name", &self.name)
            .field("<base yield>", &base_yield)
            .field("<productivity yield>", &extra_yield)
            .field("temperature", &self.temperature)
            .finish()
    }
}

impl FluidResult {
    /// 计算当前配方的实际单词产量和每次结算产能加成时的额外产量
    pub(crate) fn normalized_output(&self) -> (f64, f64) {
        let prob = self.probability;
        let ignore = match self.ignored_by_productivity {
            Some(value) => value,
            None => self.ignored_by_stats.unwrap_or(0.0),
        };
        match self.amount {
            Some(amount) => {
                let base = amount;
                let productivity = f64::max((base - ignore) * prob, 0.0);
                (base * prob, productivity)
            }
            None => {
                let min = match self.amount_min {
                    Some(value) => value,
                    None => 0.0,
                };
                let max = match self.amount_max {
                    Some(value) => value,
                    None => min,
                };
                let max = f64::max(max, min);
                let productivity = f64::max(
                    // 积分均值
                    (max - ignore + f64::max(min - ignore, 0.0))
                        * (max - f64::max(min - ignore, 0.0))
                        / 2.0
                        / (max - min)
                        * prob,
                    0.0,
                );
                (((max + min) / 2.0) * prob, productivity)
            }
        }
    }
}

const CRAFTING_MACHINE_TYPES: &[&str] = &["assembling-machine", "furnace", "rocket-silo"];

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CraftingMachinePrototype {
    #[serde(flatten)]
    pub(crate) base: PrototypeBase,

    quality_affects_energy_usage: bool,

    energy_usage: Option<EnergyAmount>,

    crafting_speed: f64,

    #[serde(deserialize_with = "as_vec_or_empty")]
    crafting_categories: Vec<String>,

    energy_source: EnergySource,

    effect_receiver: Option<EffectReceiver>,

    module_slots: f64,

    quality_affects_module_slots: bool,

    allowed_affects: Option<EffectTypeLimitation>,

    #[serde(deserialize_with = "option_as_vec_or_empty")]
    allowed_module_categories: Option<Vec<String>>,

    crafting_speed_quality_multiplier: Option<Dict<f64>>,
    module_slots_quality_bonus: Option<Dict<f64>>,
    energy_usage_quality_multiplier: Option<Dict<f64>>,

    fixed_recipe: Option<String>,
    fixed_quality: Option<String>,
    #[serde(alias = "source_inventory_size", alias = "ingredient_count")]
    input_limit: Option<f64>,
    #[serde(alias = "result_inventory_size", alias = "max_item_product_count")]
    output_limit: Option<f64>,
}
