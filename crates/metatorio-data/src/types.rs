//! 预定义类型：schema 中需要自定义反序列化的类型。
//!
//! codegen 的 `custom_type_map` 检测到这些类型名时，不生成 struct，
//! 直接引用本模块的类型（如 `crate::types::EnergyAmount`）。
//!
//! 这些类型实现**自定义 Deserialize**，因为游戏 dump 的 JSON 形态
//! 与 Rust 类型不对应（如能量是 "5MJ" 字符串而非数值）。
//!
//! 从 metatorio_egui 的 `factorio/common.rs` 与 `format.rs` 迁移而来，
//! 去掉了 egui 依赖（egui::Color32 的转换留在 UI crate）。

use std::collections::BTreeMap;

use serde::de::{Deserializer, Error as _};
use serde_json::Value;

use crate::{FluidBox, SpentFluidSpecification};

// ── 能量 ────────────────────────────────────────────────────────────

/// 能量数量，单位为焦耳（J）；功率则为焦耳每刻（J/tick）。
///
/// dump 中为字符串（如 `"5MJ"`、`"300kW"`），自定义反序列化解析为数值。
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd)]
pub struct EnergyAmount {
    pub amount: f64,
}

/// 解析能量字符串（"5MJ"、"300kW"、"1.2MW"、"45kW" 等）为焦耳。
///
/// 支持后缀：k/M/G/T/P/E/Z/Y/R/Q（10^3 递增）与 μ（10^-6）；
/// 支持功率后缀 W（÷60，焦耳每刻）；支持 J 后缀（能量）。
pub fn parse_energy(n: &str) -> Option<f64> {
    let n = n.trim();
    if n.is_empty() {
        return None;
    }
    let bytes = n.as_bytes();
    // 数字前缀（含符号与小数点），单位字母在后
    let mut num_end = 0;
    while num_end < bytes.len()
        && (bytes[num_end].is_ascii_digit()
            || bytes[num_end] == b'.'
            || bytes[num_end] == b'-'
            || bytes[num_end] == b'+')
    {
        num_end += 1;
    }
    if num_end == 0 {
        return None; // 没有数字
    }
    let numeric: f64 = n[..num_end].parse().ok()?;
    let suffix = &n[num_end..];
    let mut multiplier = match suffix.chars().next() {
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
        Some('μ') => 1e-6,
        _ => 1.0,
    };
    if suffix.ends_with('W') {
        multiplier /= 60.0;
    }
    Some(numeric * multiplier)
}

impl<'de> serde::Deserialize<'de> for EnergyAmount {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: Value = serde::Deserialize::deserialize(deserializer)?;
        match value {
            Value::String(s) => parse_energy(&s)
                .map(|amount| EnergyAmount { amount })
                .ok_or_else(|| D::Error::custom(format!("不是有效的能量字符串: {s}"))),
            Value::Number(num) => num
                .as_f64()
                .map(|amount| EnergyAmount { amount })
                .ok_or_else(|| D::Error::custom("能量数值解析失败")),
            _ => Err(D::Error::custom("能量字段既不是字符串也不是数值")),
        }
    }
}

// ── 颜色 ────────────────────────────────────────────────────────────

/// RGBA 颜色（0-255）。
///
/// dump 中为数组 `[r,g,b]`/`[r,g,b,a]`（0-1 浮点）或对象 `{r,g,b,a}`，
/// 自定义反序列化统一转为 0-255。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

impl<'de> serde::Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: Value = serde::Deserialize::deserialize(deserializer)?;
        let to_u8 = |v: &Value| -> Option<u8> { v.as_f64().map(|f| (f * 255.0).round() as u8) };
        match value {
            Value::Array(vec) => {
                // mod 数据可能给空数组/短数组（Lua 空 table 导出 {} 的另一种形态）——通道补 0
                let mut c = Color(0, 0, 0, 255);
                if vec.len() >= 1 {
                    c.0 = to_u8(&vec[0]).unwrap_or(0);
                }
                if vec.len() >= 2 {
                    c.1 = to_u8(&vec[1]).unwrap_or(0);
                }
                if vec.len() >= 3 {
                    c.2 = to_u8(&vec[2]).unwrap_or(0);
                }
                if vec.len() >= 4 {
                    c.3 = to_u8(&vec[3]).unwrap_or(255);
                }
                Ok(c)
            }
            Value::Object(object) => {
                // 空对象（Lua 空 table）→ 全 0（透明黑），不报错；a 缺失默认 255
                let get = |k: &str| -> u8 { object.get(k).and_then(to_u8).unwrap_or(0) };
                let a = object.get("a").and_then(to_u8).unwrap_or(255);
                Ok(Color(get("r"), get("g"), get("b"), a))
            }
            _ => Err(D::Error::custom("Color 不是数组或对象")),
        }
    }
}

// ── 地图位置 ────────────────────────────────────────────────────────

/// 地图位置（瓦片坐标）。
///
/// dump 中为对象 `{x, y}`（或数组 `[x, y]`），自定义反序列化统一转换。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MapPosition(pub f64, pub f64);

impl<'de> serde::Deserialize<'de> for MapPosition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: Value = serde::Deserialize::deserialize(deserializer)?;
        match value {
            Value::Object(map) => {
                let x = map
                    .get("x")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| D::Error::custom("MapPosition 缺少 x"))?;
                let y = map
                    .get("y")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| D::Error::custom("MapPosition 缺少 y"))?;
                Ok(MapPosition(x, y))
            }
            Value::Array(vec) if vec.len() >= 2 => {
                let x = vec[0]
                    .as_f64()
                    .ok_or_else(|| D::Error::custom("MapPosition 数组首元素类型错误"))?;
                let y = vec[1]
                    .as_f64()
                    .ok_or_else(|| D::Error::custom("MapPosition 数组第二元素类型错误"))?;
                Ok(MapPosition(x, y))
            }
            _ => Err(D::Error::custom("MapPosition 不是对象或长度 ≥2 的数组")),
        }
    }
}

// ── 向量与包围盒 ────────────────────────────────────────────────

/// 二维向量（schema 的 `Vector`：union[struct, tuple]，即 `{x,y}` 或 `[x,y]`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
pub struct Vector(pub f64, pub f64);

impl<'de> serde::Deserialize<'de> for Vector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: Value = serde::Deserialize::deserialize(deserializer)?;
        match value {
            Value::Object(map) => {
                let x = map.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let y = map.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
                Ok(Vector(x, y))
            }
            Value::Array(vec) if vec.len() >= 2 => {
                let x = vec[0].as_f64().unwrap_or(0.0);
                let y = vec[1].as_f64().unwrap_or(0.0);
                Ok(Vector(x, y))
            }
            _ => Err(D::Error::custom("Vector 不是对象或长度 ≥2 的数组")),
        }
    }
}

/// 三维向量（schema 的 `Vector3D`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
pub struct Vector3D(pub f64, pub f64, pub f64);

impl<'de> serde::Deserialize<'de> for Vector3D {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: Value = serde::Deserialize::deserialize(deserializer)?;
        match value {
            Value::Object(map) => {
                let x = map.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let y = map.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let z = map.get("z").and_then(|v| v.as_f64()).unwrap_or(0.0);
                Ok(Vector3D(x, y, z))
            }
            Value::Array(vec) if vec.len() >= 3 => {
                let x = vec[0].as_f64().unwrap_or(0.0);
                let y = vec[1].as_f64().unwrap_or(0.0);
                let z = vec[2].as_f64().unwrap_or(0.0);
                Ok(Vector3D(x, y, z))
            }
            _ => Err(D::Error::custom("Vector3D 不是对象或长度 ≥3 的数组")),
        }
    }
}

/// 包围盒（schema 的 `BoundingBox`：union[struct, tuple, tuple]）。
///
/// 游戏 dump 形态：`[[-0.9, -0.9], [0.9, 0.9]]`（嵌套数组）；
/// 也支持对象形态 `{left_top: {x,y}, right_bottom: {x,y}}` 与平铺 `[x1,y1,x2,y2]`。
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
pub struct BoundingBox(pub Vector, pub Vector);

impl<'de> serde::Deserialize<'de> for BoundingBox {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: Value = serde::Deserialize::deserialize(deserializer)?;
        match value {
            Value::Array(vec) if vec.len() == 2 => {
                // 嵌套形态 [[x1,y1],[x2,y2]]
                let min = serde_json::from_value::<Vector>(vec[0].clone())
                    .map_err(|_| D::Error::custom("BoundingBox 第一个顶点非法"))?;
                let max = serde_json::from_value::<Vector>(vec[1].clone())
                    .map_err(|_| D::Error::custom("BoundingBox 第二个顶点非法"))?;
                Ok(BoundingBox(min, max))
            }
            Value::Array(vec) if vec.len() >= 4 => {
                // 平铺形态 [x1, y1, x2, y2]
                Ok(BoundingBox(
                    Vector(
                        vec[0].as_f64().unwrap_or(0.0),
                        vec[1].as_f64().unwrap_or(0.0),
                    ),
                    Vector(
                        vec[2].as_f64().unwrap_or(0.0),
                        vec[3].as_f64().unwrap_or(0.0),
                    ),
                ))
            }
            Value::Object(map) => {
                let lt = map
                    .get("left_top")
                    .and_then(|v| serde_json::from_value::<Vector>(v.clone()).ok())
                    .unwrap_or_default();
                let rb = map
                    .get("right_bottom")
                    .and_then(|v| serde_json::from_value::<Vector>(v.clone()).ok())
                    .unwrap_or_default();
                Ok(BoundingBox(lt, rb))
            }
            _ => Err(D::Error::custom("BoundingBox 形态不合法")),
        }
    }
}

// ── 配方产物/原料（Product：tag 枚举，type: "item"|"fluid"）──────

/// 物品产物/原料数据（`Product::Item`）。
/// 字段全宽松（覆盖 IngredientPrototype / ProductPrototype / ItemProductPrototype）。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ItemProduct {
    pub name: String,
    #[serde(deserialize_with = "crate::lenient::de_opt_int")]
    pub amount: Option<u16>,
    #[serde(deserialize_with = "crate::lenient::de_opt_int")]
    pub amount_min: Option<u16>,
    #[serde(deserialize_with = "crate::lenient::de_opt_int")]
    pub amount_max: Option<u16>,
    #[serde(deserialize_with = "crate::lenient::de_int")]
    ignored_by_stats: u16,
    #[serde(deserialize_with = "crate::lenient::de_opt_int")]
    ignored_by_productivity: Option<u16>,
    pub extra_count_fraction: f64,
    pub quality_min: Option<String>,
    pub quality_max: Option<String>,
    #[serde(deserialize_with = "crate::lenient::de_int")]
    pub quality_change: u8,
    #[serde(default = "default_affected_by_quality")]
    pub affected_by_quality: bool,
    #[serde(default, flatten)]
    pub probability_info: ProbabilityInfo,
}

fn default_affected_by_quality() -> bool {
    true
}

/// 流体产物/原料数据（`Product::Fluid`）。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FluidProduct {
    pub name: String,
    pub amount: Option<f64>,
    pub amount_min: Option<f64>,
    pub amount_max: Option<f64>,
    pub temperature: Option<f64>,
    ignored_by_stats: f64,
    ignored_by_productivity: Option<f64>,
    #[serde(default, flatten)]
    pub probability_info: ProbabilityInfo,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProbabilityInfo {
    #[serde(default = "default_independent_probability")]
    independent_probability: f64,
    #[serde(default)]
    shared_probability: SharedProbabilityInfo,
}

fn default_independent_probability() -> f64 {
    1.0
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SharedProbabilityInfo {
    #[serde(default = "default_probability_min")]
    min: f64,
    #[serde(default = "default_probability_max")]
    max: f64,
}

fn default_probability_min() -> f64 {
    0.0
}

fn default_probability_max() -> f64 {
    1.0
}

/// 配方产物/原料（schema 的 `IngredientPrototype`/`ProductPrototype`/`ItemProductPrototype`，
/// 均为 prototypes 类——由 custom_type_map 注册）。
///
/// serde **内部标记枚举**：dump 的 `{"type": "item", "name": ..., "amount": ...}` 平铺匹配。
/// 参考 metatorio_egui `recipe.rs` 的 RecipeIngredient/RecipeResult 设计。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Product {
    Item(ItemProduct),
    Fluid(FluidProduct),
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ItemIngredient {
    pub name: String,
    #[serde(deserialize_with = "crate::lenient::de_int")]
    pub amount: u16,
    pub quality_min: Option<String>,
    pub quality_max: Option<String>,
    #[serde(deserialize_with = "crate::lenient::de_int", default)]
    pub quality_change: i8,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FluidIngredient {
    pub name: String,
    pub amount: f64,
    pub temperature: Option<f64>,
    pub minimum_temperature: Option<f64>,
    pub maximum_temperature: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Ingredient {
    Item(ItemIngredient),
    Fluid(FluidIngredient),
}

// ── 能量源（EnergySource：tag 枚举，type 字段判别）───────────────

/// 电力能量源数据（`type = "electric"`）。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ElectricEnergySource {
    pub buffer_capacity: Option<EnergyAmount>,
    pub input_flow_limit: Option<EnergyAmount>,
    pub output_flow_limit: Option<EnergyAmount>,
    pub drain: Option<EnergyAmount>,
    pub emissions_per_minute: BTreeMap<String, f64>,
}

/// 燃烧能量源数据（`type = "burner"`）。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BurnerEnergySource {
    #[serde(
        deserialize_with = "crate::lenient::de_vec_lenient",
        default = "default_fuel_categories"
    )]
    pub fuel_categories: Vec<String>,
    #[serde(default = "default_burner_usage")]
    pub burner_usage: String,
    #[serde(default = "default_effectivity")]
    pub effectivity: f64,
    pub fuel_inventory_size: Option<u32>,
    pub burnt_inventory_size: Option<u32>,
    pub emissions_per_minute: BTreeMap<String, f64>,
}

fn default_fuel_categories() -> Vec<String> {
    vec!["chemical".to_string()]
}

fn default_burner_usage() -> String {
    "fuel".to_string()
}

fn default_effectivity() -> f64 {
    1.0
}

/// 热量能量源数据（`type = "heat"`，核反应堆等）。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct HeatEnergySource {
    pub max_temperature: f64,
    pub specific_heat: EnergyAmount,
    pub emissions_per_minute: BTreeMap<String, f64>,
}

/// 流体能量源数据（`type = "fluid"`，2.0 新能量源）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FluidEnergySource {
    pub emissions_per_minute: BTreeMap<String, f64>,
    pub fluid_box: FluidBox,
    pub output_fluid_box: Option<FluidBox>,
    #[serde(default = "default_effectivity")]
    pub effectivity: f64,
    pub scale_fluid_usage: Option<bool>,
    pub fluid_usage_per_tick: f64,
    pub maximum_temperature: f64,
    pub burns_fluid: Option<bool>,
    pub spent_fluid: SpentFluidSpecification,
}

/// 能量源（schema 的 `EnergySource`：union[type × 5]）。
///
/// 机器/发电机/反应堆的能量输入方式，metatorio 计算（`energy_source_as_flow`）的核心。
/// serde **内部标记枚举**：`{"type": "electric", ...}` 直接平铺到变体数据，
/// 参考 metatorio_egui `common.rs` 的 EnergySource 设计。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum EnergySource {
    #[default]
    Void,
    Electric(ElectricEnergySource),
    Burner(BurnerEnergySource),
    Heat(HeatEnergySource),
    Fluid(FluidEnergySource),
}

// ── 科技等级上限（max_level：uint | "infinite"）──────────────────

/// 科技等级上限（schema：`union[uint32, literal "infinite"]`）。
/// 非 tag 形态（数字与字符串混用），保留自定义反序列化。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum TechnologyMaxLevel {
    U32(u32),
    Infinite,
}

impl<'de> serde::Deserialize<'de> for TechnologyMaxLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: Value = serde::Deserialize::deserialize(deserializer)?;
        match value {
            Value::Number(n) => n
                .as_u64()
                .map(|v| TechnologyMaxLevel::U32(v as u32))
                .ok_or_else(|| D::Error::custom("max_level 数字解析失败")),
            Value::String(s) if s == "infinite" => Ok(TechnologyMaxLevel::Infinite),
            _ => Err(D::Error::custom("max_level 应为数字或 \"infinite\"")),
        }
    }
}

// ── 锅炉模式（BoilerMode）────────────────────────────────────────

/// 锅炉工作模式。
///
/// schema 中是**内联 union**（`"heat-fluid-inside" | "output-to-separate-pipe"`），
/// 无法命名 → 由字段级覆盖规则（FieldRule）指定为本类型。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BoilerMode {
    /// 流体在流体箱内直接加热（默认）。
    #[default]
    HeatFluidInside,
    /// 加热后转移到独立输出管道（可设置过滤器转换流体）。
    OutputToSeparatePipe,
}

// ── 效果类型（EffectTypeLimitation）──────────────────────────────

/// 模块/信标效果类型（schema 的 union 成员是固定的 5 个字面值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EffectType {
    Speed,
    Productivity,
    Consumption,
    Pollution,
    Quality,
}

/// 允许的效果类型集合（模块/信标机器）。
///
/// schema 形态（手写建模示范——自动化无法推断语义）：
/// `union[ union[literal × 5] | array[union[literal × 5]] ]`，
/// 即"单一效果类型"或"效果类型列表"。dump 中还可能出现空表（`{}`/`[]`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectTypeLimitation {
    pub allowed: Vec<EffectType>,
}

impl<'de> serde::Deserialize<'de> for EffectTypeLimitation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: Value = serde::Deserialize::deserialize(deserializer)?;
        let mut allowed = Vec::new();
        match value {
            Value::String(s) => {
                let t = serde_json::from_value::<EffectType>(Value::String(s))
                    .map_err(|_| D::Error::custom("未知效果类型"))?;
                allowed.push(t);
            }
            Value::Array(items) => {
                for item in items {
                    if let Value::String(s) = item {
                        // 未知效果类型：容错跳过（mod 数据不规范）
                        if let Ok(t) = serde_json::from_value::<EffectType>(Value::String(s)) {
                            allowed.push(t);
                        }
                    }
                }
            }
            // 空表/空对象 → 空集
            Value::Object(_) | Value::Null => {}
            _ => return Err(D::Error::custom("EffectTypeLimitation 形态不合法")),
        }
        Ok(EffectTypeLimitation { allowed })
    }
}

impl serde::Serialize for EffectTypeLimitation {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = s.serialize_seq(Some(self.allowed.len()))?;
        for t in &self.allowed {
            seq.serialize_element(t)?;
        }
        seq.end()
    }
}

// ── Serialize（生成的组件 derive Serialize，这里按 JSON 兼容格式输出）──

impl serde::Serialize for EnergyAmount {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // 输出数值焦耳（原始字符串已丢失，保留数值语义）
        s.serialize_f64(self.amount)
    }
}

impl serde::Serialize for Color {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeTuple;
        let mut t = s.serialize_tuple(4)?;
        t.serialize_element(&(self.0 as f64 / 255.0))?;
        t.serialize_element(&(self.1 as f64 / 255.0))?;
        t.serialize_element(&(self.2 as f64 / 255.0))?;
        t.serialize_element(&(self.3 as f64 / 255.0))?;
        t.end()
    }
}

impl serde::Serialize for MapPosition {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("MapPosition", 2)?;
        st.serialize_field("x", &self.0)?;
        st.serialize_field("y", &self.1)?;
        st.end()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InventoryType {
    Normal,
    #[default]
    WithBar,
    WithFiltersAndBar,
    WithCustomStackSize,
    WithWeightLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BeaconCounter {
    #[default]
    Total,
    SameType,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct MineEntityTechnologyTrigger {
    #[serde(deserialize_with = "crate::lenient::de_vec_lenient", default)]
    entities: Vec<String>,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct CraftItemTechnologyTrigger {
    item: IDFilter,
}

#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct IDFilter {
    name: String,
    quality: Option<String>,
    comparator: Option<Comparator>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Comparator {
    #[serde(rename = "=")]
    Equal,
    #[serde(rename = "!=", alias = "≠")]
    NotEqual,
    #[serde(rename = ">")]
    GreaterThan,
    #[serde(rename = "<")]
    LessThan,
    #[serde(rename = ">=", alias = "≥")]
    GreaterThanOrEqual,
    #[serde(rename = "<=", alias = "≤")]
    LessThanOrEqual,
}

impl<'de> serde::Deserialize<'de> for IDFilter {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Value::deserialize(deserializer)? {
            Value::String(s) => Ok(IDFilter {
                name: s,
                quality: None,
                comparator: None,
            }),
            Value::Object(map) => {
                let name = map
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| D::Error::custom("ItemIDFilter 缺少 name"))?
                    .to_string();
                let quality = map
                    .get("quality")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let comparator = map
                    .get("comparator")
                    .map(Clone::clone)
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|_| D::Error::custom("Comparator 反序列化失败"))?;
                Ok(IDFilter {
                    name,
                    quality,
                    comparator,
                })
            }
            _ => Err(D::Error::custom("ItemIDFilter 不是字符串或对象")),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct CraftFluidTechnologyTrigger {
    fluid: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct SendItemToOrbitTechnologyTrigger {
    item: IDFilter,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct CaptureSpawnerTechnologyTrigger {
    entity: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct BuildEntityTechnologyTrigger {
    entity: IDFilter,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct CreateSpacePlatformTechnologyTrigger {}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum TechnologyTrigger {
    MineEntity(MineEntityTechnologyTrigger),
    CraftItem(CraftItemTechnologyTrigger),
    CraftFluid(CraftFluidTechnologyTrigger),
    SendItemToOrbit(SendItemToOrbitTechnologyTrigger),
    CaptureSpawner(CaptureSpawnerTechnologyTrigger),
    BuildEntity(BuildEntityTechnologyTrigger),
    CreateSpacePlatform(CreateSpacePlatformTechnologyTrigger),
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Modifier {
    MiningWithFluid(BoolModifer),
    SpacePlatforms(BoolModifer),
    ChangeRecipeProductivity(ChangeRecipeProductivityModifier),
    BeaconDistribution(SimpleModifier),
    BeltStackSizeBonus(SimpleModifier),
    BulkInserterCapacityBonus(SimpleModifier),
    InserterStackSizeBonus(SimpleModifier),
    LaboratoryProductivity(SimpleModifier),
    LaboratorySpeed(SimpleModifier),
    MaxCargoBayUnloadingDistance(SimpleModifier),
    MiningDrillProductivityBonus(SimpleModifier),
    UnlockQuality(UnlockQualityModifier),
    UnlockRecipe(UnlockRecipeModifier),
    UnlockSpaceLocation(UnlockSpaceLocationModifier),
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SimpleModifier {
    pub modifier: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BoolModifer {
    pub modifier: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChangeRecipeProductivityModifier {
    pub change: f64,
    pub recipe: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UnlockQualityModifier {
    pub quality: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UnlockRecipeModifier {
    pub recipe: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UnlockSpaceLocationModifier {
    pub space_location: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn energy_parsing() {
        assert_eq!(parse_energy("5MJ").unwrap(), 5_000_000.0);
        assert_eq!(parse_energy("300kW").unwrap(), 300_000.0 / 60.0);
        assert_eq!(parse_energy("45kW").unwrap(), 45_000.0 / 60.0);
        assert_eq!(parse_energy("1.2MW").unwrap(), 1_200_000.0 / 60.0);
        assert_eq!(parse_energy("100J").unwrap(), 100.0);
        assert_eq!(parse_energy("0.5k").unwrap(), 500.0);
        assert_eq!(parse_energy("abc").unwrap_or(-1.0), -1.0);
    }

    #[test]
    fn energy_deserialize() {
        let e: EnergyAmount = serde_json::from_str(r#""5MJ""#).unwrap();
        assert_eq!(e.amount, 5_000_000.0);
        // 数值兜底（非标准 dump 也可能给数字）
        let e: EnergyAmount = serde_json::from_str("123.0").unwrap();
        assert_eq!(e.amount, 123.0);
    }

    #[test]
    fn color_array_and_object() {
        let c: Color = serde_json::from_str(r#"[1.0, 0.5, 0.0]"#).unwrap();
        assert_eq!(c, Color(255, 128, 0, 255));
        let c: Color = serde_json::from_str(r#"{"r": 1.0, "g": 0.0, "b": 0.0, "a": 0.5}"#).unwrap();
        assert_eq!(c, Color(255, 0, 0, 128));
    }

    #[test]
    fn boiler_mode_parses() {
        assert_eq!(
            serde_json::from_str::<BoilerMode>(r#""heat-fluid-inside""#).unwrap(),
            BoilerMode::HeatFluidInside
        );
        assert_eq!(
            serde_json::from_str::<BoilerMode>(r#""output-to-separate-pipe""#).unwrap(),
            BoilerMode::OutputToSeparatePipe
        );
        assert!(serde_json::from_str::<BoilerMode>(r#""unknown""#).is_err());
        // 空表（Lua 空 table）不是合法模式 → 报错（由冒烟测试暴露 mod 污染）
        assert!(serde_json::from_str::<BoilerMode>(r#"{}"#).is_err());
    }

    #[test]
    fn effect_type_limitation_parses() {
        // 数组形态（dump 最常见）
        let e: EffectTypeLimitation =
            serde_json::from_str(r#"["speed", "consumption", "pollution"]"#).unwrap();
        assert_eq!(
            e.allowed,
            vec![
                EffectType::Speed,
                EffectType::Consumption,
                EffectType::Pollution
            ]
        );
        // 单值形态（union 的第一分支）
        let e: EffectTypeLimitation = serde_json::from_str(r#""quality""#).unwrap();
        assert_eq!(e.allowed, vec![EffectType::Quality]);
        // 空表 → 空集
        let e: EffectTypeLimitation = serde_json::from_str(r#"{}"#).unwrap();
        assert!(e.allowed.is_empty());
        let e: EffectTypeLimitation = serde_json::from_str(r#"[]"#).unwrap();
        assert!(e.allowed.is_empty());
        // 未知类型报错（引擎固定 5 种，mod 自定义应暴露）
        assert!(serde_json::from_str::<EffectTypeLimitation>(r#""unknown""#).is_err());
    }

    #[test]
    fn map_position_object_and_array() {
        let m: MapPosition = serde_json::from_str(r#"{"x": 1.5, "y": -2.0}"#).unwrap();
        assert_eq!(m, MapPosition(1.5, -2.0));
        let m: MapPosition = serde_json::from_str(r#"[3.0, 4.0]"#).unwrap();
        assert_eq!(m, MapPosition(3.0, 4.0));
    }

    #[test]
    fn vector_and_bounding_box_parse() {
        // Vector：对象/数组双形态
        let v: Vector = serde_json::from_str(r#"{"x": 1.5, "y": -2.0}"#).unwrap();
        assert_eq!(v, Vector(1.5, -2.0));
        let v: Vector = serde_json::from_str(r#"[3.0, 4.0]"#).unwrap();
        assert_eq!(v, Vector(3.0, 4.0));
        // BoundingBox：嵌套数组（dump 形态）/平铺/对象
        let b: BoundingBox = serde_json::from_str(r#"[[-0.9, -0.9], [0.9, 0.9]]"#).unwrap();
        assert_eq!(b, BoundingBox(Vector(-0.9, -0.9), Vector(0.9, 0.9)));
        let b: BoundingBox = serde_json::from_str(r#"[0.0, 0.0, 2.0, 2.0]"#).unwrap();
        assert_eq!(b, BoundingBox(Vector(0.0, 0.0), Vector(2.0, 2.0)));
        let b: BoundingBox = serde_json::from_str(
            r#"{"left_top": {"x": 0.0, "y": 0.0}, "right_bottom": {"x": 1.0, "y": 1.0}}"#,
        )
        .unwrap();
        assert_eq!(b, BoundingBox(Vector(0.0, 0.0), Vector(1.0, 1.0)));
    }

    #[test]
    fn product_tag_enum_parses() {
        // 物品产物（tag = "item"，平铺）
        let p: Product =
            serde_json::from_str(r#"{"type": "item", "name": "iron-plate", "amount": 1}"#).unwrap();
        match &p {
            Product::Item(data) => {
                assert_eq!(data.name, "iron-plate");
                assert_eq!(data.amount, Some(1));
            }
            Product::Fluid(_) => panic!("应为物品产物"),
        }
        // 流体原料（带温度字段）
        let p: Product = serde_json::from_str(
            r#"{"type": "fluid", "name": "water", "amount": 10, "temperature": 15}"#,
        )
        .unwrap();
        match &p {
            Product::Fluid(data) => {
                assert_eq!(data.name, "water");
                assert_eq!(data.temperature, Some(15.0));
            }
            Product::Item(_) => panic!("应为流体原料"),
        }
        // 缺字段容错（#[serde(default)]）
        let p: Product = serde_json::from_str(r#"{"type": "item", "name": "x"}"#).unwrap();
        assert!(matches!(p, Product::Item(_)));
        // 未知 type 报错
        assert!(serde_json::from_str::<Product>(r#"{"type": "unknown", "name": "x"}"#).is_err());
    }

    #[test]
    fn energy_source_tag_enum_parses() {
        // electric：平铺
        let e: EnergySource = serde_json::from_str(
            r#"{"type": "electric", "buffer_capacity": "5MJ", "usage_priority": "tertiary"}"#,
        )
        .unwrap();
        match &e {
            EnergySource::Electric(data) => {
                assert_eq!(data.buffer_capacity.unwrap().amount, 5_000_000.0);
            }
            _ => panic!("应为 electric"),
        }
        // burner
        let e: EnergySource = serde_json::from_str(
            r#"{"type": "burner", "fuel_categories": ["chemical"], "effectivity": 1.0}"#,
        )
        .unwrap();
        assert!(matches!(e, EnergySource::Burner(_)));
        // void（unit 变体）
        let e: EnergySource = serde_json::from_str(r#"{"type": "void"}"#).unwrap();
        assert!(matches!(e, EnergySource::Void));
        // 缺 type 字段 → 报错
        assert!(serde_json::from_str::<EnergySource>(r#"{"buffer_capacity": "5MJ"}"#).is_err());
        // 未知 type 报错
        assert!(serde_json::from_str::<EnergySource>(r#"{"type": "nuclear"}"#).is_err());
    }

    #[test]
    fn technology_max_level_parses() {
        assert_eq!(
            serde_json::from_str::<TechnologyMaxLevel>("5").unwrap(),
            TechnologyMaxLevel::U32(5)
        );
        assert_eq!(
            serde_json::from_str::<TechnologyMaxLevel>(r#""infinite""#).unwrap(),
            TechnologyMaxLevel::Infinite
        );
        assert!(serde_json::from_str::<TechnologyMaxLevel>(r#""5""#).is_err());
    }
}
