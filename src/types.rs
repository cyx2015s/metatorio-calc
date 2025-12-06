// 尽可能忠实地反应原始 JSON 结构，同时提供方便的访问方法。

use std::{cmp::Ordering, collections::HashMap, fmt::Debug};

use serde::{de::DeserializeOwned, *};

use serde_json::{Value, from_value};

fn as_vec_or_empty<'de, T, D>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::Array(vec) => Ok(from_value(Value::Array(vec)).map_err(serde::de::Error::custom)?),
        Value::Object(map) if map.is_empty() => Ok(Vec::new()),
        _ => Err(serde::de::Error::custom("不是数组或空对象。")),
    }
}

fn option_as_vec_or_empty<'de, T, D>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let value = as_vec_or_empty(deserializer);
    match value {
        Ok(vec) => Ok(Some(vec)),
        Err(_) => Ok(None),
    }
}

fn floored<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: TryFrom<u64>,
    <T as TryFrom<u64>>::Error: std::fmt::Display,
{
    let value = serde_json::Value::deserialize(deserializer);
    match value {
        Result::Ok(value) => match value {
            serde_json::Value::Number(num) => {
                if let Some(int) = num.as_u64() {
                    T::try_from(int).map_err(de::Error::custom)
                } else if let Some(float) = num.as_f64() {
                    if !float.is_finite() || float.is_sign_negative() {
                        return Err(de::Error::custom("不是有效的非负数字"));
                    }
                    let floored = float.floor() as u64;
                    T::try_from(floored).map_err(de::Error::custom)
                } else {
                    Err(de::Error::custom("不是数字"))
                }
            }
            _ => Err(de::Error::custom("不是数字")),
        },
        _ => Err(de::Error::custom("无法反序列化值")),
    }
}

fn option_floored<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: TryFrom<u64>,
    <T as TryFrom<u64>>::Error: std::fmt::Display,
{
    let value = floored(deserializer);
    match value {
        Ok(v) => Ok(Some(v)),
        Err(_) => Ok(None),
    }
}
/// 如果某个原型除了叫什么，名字是什么，分组是什么之外其他都不关心（不影响量化计算）
/// 可以使用这个结构体来简化反序列化。
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub(crate) struct InformativePrototype {
    name: String,
    order: String,
    subgroup: Option<String>,
    hidden: bool,
    parameter: bool,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub(crate) struct RecipePrototype {
    /// 配方 ID
    name: String,
    order: String,
    subgroup: Option<String>,
    hidden: bool,
    parameter: bool,

    /// 类别
    category: String,

    /// 额外类别
    #[serde(deserialize_with = "as_vec_or_empty")]
    additional_categories: Vec<String>,

    /// 配方原料
    #[serde(deserialize_with = "as_vec_or_empty")]
    pub(crate) ingredients: Vec<RecipeIngredient>,

    /// 配方产出
    #[serde(deserialize_with = "as_vec_or_empty")]
    pub(crate) results: Vec<RecipeResult>,

    /// 允许的插件类别，为空表示所有，但仍受配方本身的加成限制
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

impl PartialEq for RecipePrototype {
    fn eq(&self, other: &Self) -> bool {
        (&self.order, &self.name) == (&other.order, &other.name)
    }
}

impl PartialOrd for RecipePrototype {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        (&self.order, &self.name).partial_cmp(&(&other.order, &other.name))
    }
}

impl Default for RecipePrototype {
    fn default() -> Self {
        RecipePrototype {
            name: "recipe-unknown".to_string(),
            order: String::new(),
            category: "crafting".to_string(),
            additional_categories: vec![],
            ingredients: vec![],
            results: vec![],
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
            allowed_module_categories: None,
            subgroup: None,
            hidden: false,
            parameter: false,
        }
    }
}

#[allow(dead_code)]
impl RecipePrototype {
    /// 获取所有类别，包括主类别和额外类别
    pub(crate) fn categories(&self) -> Vec<String> {
        let mut categories = vec![self.category.clone()];
        categories.extend(self.additional_categories.clone());
        categories
    }
}

// Ingredients and Results
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
pub(crate) enum RecipeIngredient {
    item(ItemIngredient),
    fluid(FluidIngredient),
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub(crate) struct ItemIngredient {
    /// 物品 ID
    pub(crate) name: String,

    /// 消耗数量
    #[serde(deserialize_with = "floored")]
    pub(crate) amount: u16,
}

impl Default for ItemIngredient {
    fn default() -> Self {
        ItemIngredient {
            name: "item-unknown".to_string(),
            amount: 0,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub(crate) struct FluidIngredient {
    /// 流体 ID
    pub(crate) name: String,

    /// 流体数量
    pub(crate) amount: f64,

    /// 默认温度为流体的最低温度，与流体原型有关
    temperature: Option<f64>,

    /// 限制最低温度
    min_temperature: Option<f64>,

    /// 限制最高温度
    max_temperature: Option<f64>,
    fluidbox_index: u32,
}
impl Default for FluidIngredient {
    fn default() -> Self {
        FluidIngredient {
            name: "fluid-unknown".to_string(),
            amount: 0.0,
            temperature: None,
            min_temperature: None,
            max_temperature: None,
            fluidbox_index: 0,
        }
    }
}
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
pub(crate) enum RecipeResult {
    item(ItemResult),
    fluid(FluidResult),
}

#[derive(Deserialize)]
#[serde(default)]
pub(crate) struct ItemResult {
    /// 物品 ID
    pub(crate) name: String,

    /// 产出物品数量
    #[serde(deserialize_with = "option_floored")]
    pub(crate) amount: Option<u16>,
    /// 仅在 amount = nil 时读取，最小可能产出数量
    #[serde(deserialize_with = "option_floored")]
    amount_min: Option<u16>,
    /// 仅在 amount = nil 时读取，最大可能产出数量
    #[serde(deserialize_with = "option_floored")]
    amount_max: Option<u16>,

    /// 与 [ItemResult::amount]或[ItemResult::amount_min]/[ItemResult::amount_max] 结合使用，表示产出概率
    probability: f64,

    /// 统计数据时的忽略产出数量，不影响产能加成但是会影响产能加成的默认值
    #[serde(deserialize_with = "option_floored")]
    ignored_by_stats: Option<u16>,

    /// 产能条走慢时，忽略产能部分的数量
    #[serde(deserialize_with = "option_floored")]
    ignored_by_productivity: Option<u16>,

    /// 每一次产出时，额外产出一个的概率，也会收到[ItemResult::ignored_by_productivity]影响
    extra_count_fraction: f32,

    /// 可变质物品的变质进度
    percent_spoiled: f32,
}

impl Default for ItemResult {
    fn default() -> Self {
        ItemResult {
            name: "item-unknown".to_string(),
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
        let extra = self.extra_count_fraction as f64;
        let prob = self.probability;
        let ignore = match self.ignored_by_productivity {
            Some(value) => value as f64,
            None => self.ignored_by_stats.unwrap_or(0) as f64,
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

#[derive(Deserialize)]
#[serde(default)]
pub(crate) struct FluidResult {
    /// 流体 ID
    pub(crate) name: String,

    /// 流体产出数量
    amount: Option<f64>,
    /// 仅在 amount = 0 时读取，最小可能产出数量
    amount_min: Option<f64>,
    /// 仅在 amount = 0 时读取，最大可能产出数量
    amount_max: Option<f64>,
    /// 与 [FluidResult::amount]或[FluidResult::amount_min]/[FluidResult::amount_max] 结合使用，表示产出概率
    probability: f64,
    /// 统计数据时的忽略产出数量，不影响产能加成但是会影响产能加成的默认值
    ignored_by_stats: Option<f64>,
    /// 产能条走慢时，忽略产能部分的数量
    ignored_by_productivity: Option<f64>,
    /// 流体输出温度
    temperature: Option<f32>,
    fluidbox_index: u32,
}

impl Default for FluidResult {
    fn default() -> Self {
        FluidResult {
            name: "fluid-unknown".to_string(),
            amount: None,
            amount_min: None,
            amount_max: None,
            probability: 1.0,
            ignored_by_stats: None,
            ignored_by_productivity: None,
            temperature: None,
            fluidbox_index: 0,
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

#[derive(Deserialize, Default)]
pub(crate) struct EnergyAmount {
    /// 每一刻消耗的能量（焦耳）
    /// 用作功率时，乘以60得到瓦特数
    amount: f64,
}

impl Debug for EnergyAmount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} J (or {} W)", self.amount, self.amount * 60.0)
    }
}

fn as_energy<'de, D>(deserializer: D) -> Result<Option<EnergyAmount>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer);
    match value {
        Result::Ok(value) => match value {
            serde_json::Value::Null => Ok(None),
            serde_json::Value::Number(num) => Ok(Some(EnergyAmount {
                amount: num
                    .as_f64()
                    .ok_or_else(|| de::Error::custom("不是有效数字"))?,
            })),
            serde_json::Value::String(s) => {
                let re = regex::Regex::new(r"^[\d|.]+[k|M|G|T|P|E|Z|Y|R|Q]?[J|W]?$")
                    .map_err(de::Error::custom)?;
                if re.is_match(&s) {
                    let mut multiplier = match s.chars().rev().nth(1) {
                        Some('k') => 1_000.0,
                        Some('M') => 1_000_000.0,
                        Some('G') => 1_000_000_000.0,
                        Some('T') => 1_000_000_000_000.0,
                        Some('P') => 1_000_000_000_000_000.0,
                        Some('E') => 1_000_000_000_000_000_000.0,
                        Some('Z') => 1_000_000_000_000_000_000_000.0,
                        Some('Y') => 1_000_000_000_000_000_000_000_000.0,
                        Some('R') => 1_000_000_000_000_000_000_000_000_000.0,
                        Some('Q') => 1_000_000_000_000_000_000_000_000_000_000.0,
                        _ => 1.0,
                    };
                    let dimension_char = s.chars().last();
                    match dimension_char {
                        Some('W') => multiplier /= 60.0,
                        _ => {}
                    }
                    let value: f64 = s
                        .trim_end_matches(|c: char| !c.is_digit(10))
                        .parse()
                        .map_err(de::Error::custom)?;
                    Ok(Some(EnergyAmount {
                        amount: value * multiplier,
                    }))
                } else {
                    Err(de::Error::custom(format!("不是有效的能量字符串: {}", &s)))
                }
            }
            _ => Err(de::Error::custom("不是数字或字符串")),
        },
        _ => Err(de::Error::custom("无法反序列化值")),
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
pub(crate) enum EnergySource {
    electric(ElectricEnergySource),
    burner(BurnerEnergySource),
    heat(HeatEnergySource),
    fluid(FluidEnergySource),
    void(VoidEnergySource),
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub(crate) struct ElectricEnergySource {
    #[serde(deserialize_with = "as_energy")]
    buffer_capacity: Option<EnergyAmount>,

    #[serde(deserialize_with = "as_energy")]
    input_flow_limit: Option<EnergyAmount>,

    #[serde(deserialize_with = "as_energy")]
    output_flow_limit: Option<EnergyAmount>,

    #[serde(deserialize_with = "as_energy")]
    drain: Option<EnergyAmount>,
    emissions_per_minute: Option<HashMap<String, f64>>,
}

impl Default for ElectricEnergySource {
    fn default() -> Self {
        ElectricEnergySource {
            buffer_capacity: None,
            input_flow_limit: None,
            output_flow_limit: None,
            drain: None,
            emissions_per_minute: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub(crate) struct BurnerEnergySource {
    burnt_inventory_size: u16,
    effectivity: f64,
    burner_usage: String,
    emissions_per_minute: Option<HashMap<String, f64>>,
}

impl Default for BurnerEnergySource {
    fn default() -> Self {
        BurnerEnergySource {
            burnt_inventory_size: 0,
            effectivity: 1.0,
            burner_usage: "fuel".to_string(),
            emissions_per_minute: None,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub(crate) struct HeatEnergySource {
    max_temperature: f64,
    emissions_per_minute: Option<HashMap<String, f64>>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub(crate) struct FluidEnergySource {
    effectivity: f64,
    fluid_usage_per_tickop: f64,
    scale_fluid_usage: bool,
    burns_fluid: bool,
    emissions_per_minute: Option<HashMap<String, f64>>,
}
impl Default for FluidEnergySource {
    fn default() -> Self {
        FluidEnergySource {
            effectivity: 1.0,
            fluid_usage_per_tickop: 0.0,
            scale_fluid_usage: false,
            burns_fluid: false,
            emissions_per_minute: None,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub(crate) struct VoidEnergySource {
    emissions_per_minute: Option<HashMap<String, f64>>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub(crate) struct EffectReceiver {
    base_effect: Effect,
    use_module_effects: bool,
    use_beacon_effects: bool,
    use_surface_effects: bool,
}

impl Default for EffectReceiver {
    fn default() -> Self {
        EffectReceiver {
            base_effect: Effect::default(),
            use_module_effects: true,
            use_beacon_effects: true,
            use_surface_effects: true,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub(crate) struct Effect {
    consumption: f32,
    speed: f32,
    productivity: f32,
    efficiency: f32,
    quality: f32,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
#[serde(untagged)]
pub(crate) enum EffectTypeLimitation {
    Single(String),
    Multiple(Vec<String>),
    Empty(HashMap<String, Value>),
}

impl Default for EffectTypeLimitation {
    fn default() -> Self {
        EffectTypeLimitation::Multiple(vec![])
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub(crate) struct CraftingMachinePrototype {
    name: String,
    order: String,
    subgroup: Option<String>,
    hidden: bool,
    parameter: bool,

    quality_affects_energy_usage: bool,

    #[serde(deserialize_with = "as_energy")]
    energy_usage: Option<EnergyAmount>,
    crafting_speed: f64,

    #[serde(deserialize_with = "as_vec_or_empty")]
    crafting_categories: Vec<String>,
    energy_source: Option<EnergySource>,
    effect_receiver: Option<EffectReceiver>,
    #[serde(deserialize_with = "floored")]
    module_slots: u16,
    quality_affects_module_slots: bool,

    allowed_effects: Option<EffectTypeLimitation>,
    #[serde(deserialize_with = "option_as_vec_or_empty")]
    allowed_module_categories: Option<Vec<String>>,

    // Quality related
    crafting_speed_quality_multiplier: Option<HashMap<String, f32>>,
    module_slots_quality_bonus: Option<HashMap<String, u32>>,
    energy_usage_quality_multiplier: Option<HashMap<String, f32>>,

    // Assembler specific
    fixed_recipe: Option<String>,
    fixed_quality: Option<String>,
    #[serde(alias = "source_inventory_size", alias = "ingredient_count")]
    input_limit: Option<u32>,
    #[serde(alias = "result_inventory_size", alias = "max_item_product_count")]
    output_limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub(crate) struct ItemPrototype {
    name: String,
    order: Option<String>,
    subgroup: Option<String>,
    hidden: bool,
    parameter: bool,

    stack_size: u32,

    // 放置实体
    place_result: Option<String>,

    // 燃料行为
    fuel_category: Option<String>,
    burnt_result: Option<String>,
    #[serde(deserialize_with = "as_energy")]
    fuel_value: Option<EnergyAmount>,

    // 变质行为
    spoil_result: Option<String>,
    #[serde(deserialize_with = "option_floored")]
    spoil_ticks: Option<u32>,

    // 种植行为
    plant_result: Option<String>,

    // 放置地格
    // place_as_tile: Option<String>,

    // 火箭发射
    #[serde(deserialize_with = "option_as_vec_or_empty")]
    rocket_launch_products: Option<Vec<ItemResult>>,

    #[serde(deserialize_with = "option_as_vec_or_empty")]
    flags: Option<Vec<String>>,

    // 插件效果
    effect: Option<Effect>,
    category: Option<String>,
}

impl Default for ItemPrototype {
    fn default() -> Self {
        ItemPrototype {
            name: "item-unknown".to_string(),
            stack_size: 0,
            place_result: None,
            fuel_category: None,
            burnt_result: None,
            spoil_result: None,
            plant_result: None,
            // place_as_tile: None,
            flags: None,
            fuel_value: None,
            spoil_ticks: None,
            rocket_launch_products: None,
            category: None,
            effect: None,
            order: None,
            subgroup: None,
            hidden: false,
            parameter: false,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub(crate) struct FluidPrototype {
    name: String,
    order: String,
    subgroup: Option<String>,
    hidden: bool,
    parameter: bool,

    default_temperature: f64,
    max_temperature: Option<f64>,

    emissions_multiplier: f64,

    #[serde(deserialize_with = "as_energy")]
    heat_capacity: Option<EnergyAmount>,
    #[serde(deserialize_with = "as_energy")]
    fuel_value: Option<EnergyAmount>,
    fuel_category: Option<String>,
}

impl Default for FluidPrototype {
    fn default() -> Self {
        FluidPrototype {
            name: "fluid-unknown".to_string(),
            order: String::new(),
            default_temperature: 15.0,
            max_temperature: None,
            heat_capacity: Some(EnergyAmount { amount: 1000.0 }),
            fuel_value: Some(EnergyAmount { amount: 0.0 }),
            fuel_category: None,
            subgroup: None,
            emissions_multiplier: 0.0,
            hidden: false,
            parameter: false,
        }
    }
}
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub(crate) struct MinableProperties {
    mining_time: f64,
    result: Option<String>,
    #[serde(deserialize_with = "option_floored")]
    count: Option<u16>,
    #[serde(deserialize_with = "option_as_vec_or_empty")]
    results: Option<Vec<RecipeResult>>,

    fluid_amount: f64,
    required_fluid: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub(crate) struct LootItem {
    item: String,
    probability: f64,
    count_min: u16,
    count_max: u16,
}

impl Default for LootItem {
    fn default() -> Self {
        LootItem {
            item: "item-unknown".to_string(),
            probability: 1.0,
            count_min: 1,
            count_max: 1,
        }
    }
}

/// 表示所有能够产出资源的实体，可能与 MachinePrototype 重叠，不过无所谓了，毕竟被转换的物体本身是什么类型可以很多变的
#[derive(Debug, Deserialize)]
#[serde(default)]
pub(crate) struct ResourceEntityPrototype {
    // 类型太多了，得手动区分一下
    r#type: String,
    name: String,
    order: String,
    subgroup: Option<String>,
    hidden: bool,
    parameter: bool,

    /// 击杀掉落物，也用作植物的收获掉落物
    pub(crate) loot: Option<Vec<LootItem>>,

    /// 出现于Entity、AsteroidChunk和Tile的定义中
    pub(crate) minable: Option<MinableProperties>,
    pub(crate) category: String,

    /// 出现于植物的定义中
    #[serde(deserialize_with = "floored")]
    growth_ticks: u64,
    harvest_emissions: Option<HashMap<String, f64>>,
}

impl Default for ResourceEntityPrototype {
    fn default() -> Self {
        ResourceEntityPrototype {
            r#type: String::new(),
            name: "entity-unknown".to_string(),
            order: String::new(),
            subgroup: None,
            hidden: false,
            parameter: false,
            minable: None,
            category: "basic-solid".to_string(),
            growth_ticks: 0,
            harvest_emissions: None,
            loot: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub(crate) struct LabPrototype {
    name: String,
    order: String,
    subgroup: Option<String>,
    hidden: bool,
    parameter: bool,

    #[serde(deserialize_with = "as_energy")]
    energy_usage: Option<EnergyAmount>,

    energy_source: Option<EnergySource>,

    /// 接受的科技包类型
    #[serde(deserialize_with = "as_vec_or_empty")]
    inputs: Vec<String>,

    researching_speed: f64,

    effect_receiver: Option<EffectReceiver>,

    #[serde(deserialize_with = "floored")]
    module_slots: u16,

    quality_affects_module_slots: bool,
    uses_quality_drain_modifier: bool,
    science_pack_drain_rate_percent: u8,
    allowed_effects: Option<EffectTypeLimitation>,
    #[serde(deserialize_with = "option_as_vec_or_empty")]
    allowed_module_categories: Option<Vec<String>>,
}

impl Default for LabPrototype {
    fn default() -> Self {
        LabPrototype {
            name: "entity-unknown".to_string(),
            order: String::new(),
            subgroup: None,
            hidden: false,
            parameter: false,
            energy_usage: None,
            energy_source: None,
            inputs: vec![],
            researching_speed: 1.0,
            effect_receiver: None,
            module_slots: 0,
            quality_affects_module_slots: false,
            uses_quality_drain_modifier: false,
            science_pack_drain_rate_percent: 100,
            allowed_effects: None,
            allowed_module_categories: None,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct FluidBox {
    filter: Option<String>,
    minimum_temperature: Option<f64>,
    maximum_temperature: Option<f64>,
    production_type: Option<String>,
}

/// 包括：采矿机，星岩抓取臂，农业塔，抽水泵
#[derive(Debug, Deserialize)]
#[serde(default)]
pub(crate) struct MiningDrillPrototype {
    name: String,
    order: String,
    subgroup: Option<String>,
    hidden: bool,
    parameter: bool,

    r#type: String,

    /// 采矿机限定属性
    resource_categories: Vec<String>,
    mining_speed: f64,
    #[serde(alias = "fluid_box")]
    input_fluid_box: Option<FluidBox>, // 占位用，不需要具体内容
    output_fluid_box: Option<FluidBox>, // 占位用，不需要具体内容
    #[serde(deserialize_with = "as_energy")]
    #[serde(alias = "energy_consumption ")]
    energy_usage: Option<EnergyAmount>,
    /// 也用于农业塔和抽水泵
    /// 对于发电机而言，指定了发电的属性和电量
    energy_source: Option<EnergySource>,
    effect_receiver: Option<EffectReceiver>,
    #[serde(deserialize_with = "floored")]
    module_slots: u16,
    quality_affects_module_slots: bool,
    allowed_effects: Option<EffectTypeLimitation>,

    #[serde(deserialize_with = "option_as_vec_or_empty")]
    allowed_module_categories: Option<Vec<String>>,

    /// 星岩抓取臂限定属性，实际上不好量化
    #[serde(deserialize_with = "as_energy")]
    passive_energy_usage: Option<EnergyAmount>,
    #[serde(deserialize_with = "as_energy")]
    arm_energy_usage: Option<EnergyAmount>,

    /// 农业塔限定属性
    radius: Option<f64>,
    #[serde(deserialize_with = "option_floored")]
    growth_grid_tile_size: Option<u32>,
    #[serde(deserialize_with = "option_floored")]
    input_inventory_size: Option<u16>,
    #[serde(deserialize_with = "option_floored")]
    output_inventory_size: Option<u16>,

    /// 抽水泵限定属性
    pumping_speed: Option<f64>,

    /// 锅炉限定属性
    mode: Option<String>,
    target_temperature: Option<f64>,

    /// 热能发电机限定属性
    burner: Option<BurnerEnergySource>,
    #[serde(deserialize_with = "as_energy")]
    max_power_output: Option<EnergyAmount>,

    /// 聚变反应堆限定属性
    max_fluid_usage: Option<f64>,
}

impl Default for MiningDrillPrototype {
    fn default() -> Self {
        MiningDrillPrototype {
            name: "entity-unknown".to_string(),
            order: String::new(),
            subgroup: None,
            hidden: false,
            parameter: false,
            resource_categories: vec![],
            energy_usage: None,
            energy_source: None,
            mining_speed: 1.0,
            effect_receiver: None,
            module_slots: 0,
            quality_affects_module_slots: false,
            allowed_effects: None,
            allowed_module_categories: None,
            r#type: String::new(),
            passive_energy_usage: None,
            arm_energy_usage: None,
            growth_grid_tile_size: Some(3),
            input_inventory_size: None,
            output_inventory_size: None,
            input_fluid_box: None,
            output_fluid_box: None,
            radius: None,
            pumping_speed: None,
            mode: None,
            target_temperature: None,
            burner: None,
            max_power_output: None,
            max_fluid_usage: None,
        }
    }
}
