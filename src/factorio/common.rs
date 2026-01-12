use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    hash::Hash,
    ops::Add,
};

use serde::{Deserialize, Deserializer, de::DeserializeOwned};
use serde_json::{Value, from_value};

use crate::factorio::model::context::{FactorioContext, GenericItem};

pub type Dict<T> = HashMap<String, T>;
pub type Emissions = Dict<f64>;
pub type OrderInfo = Vec<(String, Vec<(String, Vec<String>)>)>;
pub type ReverseOrderInfo = HashMap<String, (usize, usize, usize)>;

#[derive(Debug, Clone)]
pub struct IdWithQuality(pub String, pub u8);

impl From<String> for IdWithQuality {
    fn from(s: String) -> Self {
        IdWithQuality(s, 0)
    }
}

impl From<&str> for IdWithQuality {
    fn from(s: &str) -> Self {
        IdWithQuality(s.to_string(), 0)
    }
}

impl From<(String, u8)> for IdWithQuality {
    fn from(t: (String, u8)) -> Self {
        IdWithQuality(t.0, t.1)
    }
}

pub fn update_map<T, N>(map: &mut HashMap<T, N>, key: T, value: N)
where
    T: Hash + Eq,
    N: Add<Output = N> + Copy + Default,
{
    let entry = map.entry(key).or_default();
    *entry = *entry + value;
}

pub fn version_string_to_triplet(version: &str) -> (u16, u16, u16) {
    let parts: Vec<&str> = version.split('.').collect();
    let major = parts
        .first()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let minor = parts
        .get(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let patch = parts
        .get(2)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    (major, minor, patch)
}

#[derive(Debug, Clone)]
pub struct Color(u8, u8, u8, u8);

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: Value = Deserialize::deserialize(deserializer)?;
        match value {
            Value::Array(vec) => {
                if vec.len() < 3 {
                    return Err(serde::de::Error::custom("Color 数组长度不为 3 或 4"));
                }
                let r = (vec[0]
                    .as_f64()
                    .ok_or_else(|| serde::de::Error::custom("Color 数组第一个元素类型错误"))?
                    * 255.0) as u8;
                let g = (vec[1]
                    .as_f64()
                    .ok_or_else(|| serde::de::Error::custom("Color 数组第二个元素类型错误"))?
                    * 255.0) as u8;
                let b = (vec[2]
                    .as_f64()
                    .ok_or_else(|| serde::de::Error::custom("Color 数组第三个元素类型错误"))?
                    * 255.0) as u8;
                let a = if vec.len() >= 4 {
                    (vec[3]
                        .as_f64()
                        .ok_or_else(|| serde::de::Error::custom("Color 数组第四个元素类型错误"))?
                        * 255.0) as u8
                } else {
                    255
                };
                Ok(Color(r, g, b, a))
            }
            Value::Object(object) => {
                let r =
                    (object.get("r").and_then(|v| v.as_f64()).ok_or_else(|| {
                        serde::de::Error::custom("Color 结构体缺少 r 字段或类型错误")
                    })? * 255.0) as u8;
                let g =
                    (object.get("g").and_then(|v| v.as_f64()).ok_or_else(|| {
                        serde::de::Error::custom("Color 结构体缺少 g 字段或类型错误")
                    })? * 255.0) as u8;
                let b =
                    (object.get("b").and_then(|v| v.as_f64()).ok_or_else(|| {
                        serde::de::Error::custom("Color 结构体缺少 b 字段或类型错误")
                    })? * 255.0) as u8;
                let a = if let Some(alpha_value) = object.get("a") {
                    (alpha_value
                        .as_f64()
                        .ok_or_else(|| serde::de::Error::custom("Color 结构体的 a 字段类型错误"))?
                        * 255.0) as u8
                } else {
                    255
                };
                Ok(Color(r, g, b, a))
            }
            _ => Err(serde::de::Error::custom("Color 不是数组类型")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MapPosition(pub f64, pub f64);

impl<'de> Deserialize<'de> for MapPosition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: Value = Deserialize::deserialize(deserializer)?;
        match value {
            Value::Object(map) => {
                let x = map.get("x").and_then(|v| v.as_f64()).ok_or_else(|| {
                    serde::de::Error::custom("MapPosition 结构体缺少 x 字段或类型错误")
                })?;
                let y = map.get("y").and_then(|v| v.as_f64()).ok_or_else(|| {
                    serde::de::Error::custom("MapPosition 结构体缺少 y 字段或类型错误")
                })?;
                Ok(MapPosition(x, y))
            }
            Value::Array(vec) => {
                if vec.len() < 2 {
                    return Err(serde::de::Error::custom("MapPosition 数组长度不为 2"));
                }
                let x = vec[0].as_f64().ok_or_else(|| {
                    serde::de::Error::custom("MapPosition 数组第一个元素类型错误")
                })?;
                let y = vec[1].as_f64().ok_or_else(|| {
                    serde::de::Error::custom("MapPosition 数组第二个元素类型错误")
                })?;
                Ok(MapPosition(x, y))
            }
            _ => Err(serde::de::Error::custom(
                "MapPosition 既不是结构体也不是数组",
            )),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum BoundingBox {
    Struct {
        left_top: MapPosition,
        right_bottom: MapPosition,
        orientation: Option<f64>,
    },
    Pair(MapPosition, MapPosition),
    Triplet(MapPosition, MapPosition, f64),
}

pub fn as_vec_or_empty<'de, T, D>(deserializer: D) -> Result<Vec<T>, D::Error>
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

pub fn option_as_vec_or_empty<'de, T, D>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
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

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
/// PrototypeBase 基类中我们关心的字段
#[derive(Default)]
pub struct PrototypeBase {
    /// 类型名
    pub r#type: String,
    /// 内部名
    pub name: String,
    /// 排序依据
    pub order: String,
    /// 子组
    pub subgroup: String,
    /// 默认隐藏
    pub hidden: bool,
    /// 视为参数
    pub parameter: bool,
}

pub trait HasPrototypeBase {
    fn base(&self) -> &PrototypeBase;
}

impl HasPrototypeBase for PrototypeBase {
    fn base(&self) -> &PrototypeBase {
        self
    }
}

#[derive(Debug, Clone)]
/// 能量数量，单位为焦耳（J），如果是功率则为焦耳每刻（J/tick）
pub struct EnergyAmount {
    pub amount: f64,
}

impl<'de> Deserialize<'de> for EnergyAmount {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value: String = Deserialize::deserialize(deserializer)?;
        let re = regex::Regex::new(r"^[\d|.]+[k|M|G|T|P|E|Z|Y|R|Q]?[J|W]?$")
            .map_err(serde::de::Error::custom)?;
        if re.is_match(&value) {
            let mut multiplier = match value.chars().rev().nth(1) {
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
            let dimension_char = value.chars().last();
            if let Some('W') = dimension_char {
                multiplier /= 60.0
            }
            let numeric_value: f64 = value
                .trim_end_matches(|c: char| !c.is_ascii_digit())
                .parse()
                .map_err(serde::de::Error::custom)?;
            Ok(EnergyAmount {
                amount: numeric_value * multiplier,
            })
        } else {
            Err(serde::de::Error::custom(format!(
                "不是有效的能量字符串: {}",
                &value
            )))
        }
    }
}

const ENERGY_SUFFIX: &str = " kMGTPEZYRQ";

impl Display for EnergyAmount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut power = 0;
        let mut divisor = 1.0;
        while self.amount >= divisor * 1000.0 && power < ENERGY_SUFFIX.len() {
            divisor *= 1000.0;
            power += 1;
        }
        write!(
            f,
            "{}{}J",
            f64::round(self.amount / divisor * 100.0) / 100.0,
            ENERGY_SUFFIX.chars().nth(power).unwrap()
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum EnergySource {
    #[serde(rename = "electric")]
    Electric(ElectricEnergySource),
    #[serde(rename = "burner")]
    Burner(BurnerEnergySource),
    #[serde(rename = "heat")]
    Heat(HeatEnergySource),
    #[serde(rename = "fluid")]
    Fluid(FluidEnergySource),
    #[serde(rename = "void")]
    Void(VoidEnergySource),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct ElectricEnergySource {
    buffer_capacity: Option<EnergyAmount>,
    input_flow_limit: Option<EnergyAmount>,
    output_flow_limit: Option<EnergyAmount>,
    pub drain: Option<EnergyAmount>,
    pub emissions_per_minute: Option<Emissions>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BurnerEnergySource {
    pub burnt_inventory_size: f64,
    pub effectivity: f64,
    pub burner_usage: String,
    pub emissions_per_minute: Option<Dict<f64>>,
}

impl Default for BurnerEnergySource {
    fn default() -> Self {
        BurnerEnergySource {
            burnt_inventory_size: 0.0,
            effectivity: 1.0,
            burner_usage: "fuel".to_string(),
            emissions_per_minute: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct HeatEnergySource {
    pub max_temperature: f64,
    pub emissions_per_minute: Option<Dict<f64>>,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FluidIOMode {
    #[default]
    None,
    Input,
    InputOutput,
    Output,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FluidBox {
    #[serde(default)]
    pub filter: Option<String>,
    pub minimum_temperature: Option<f64>,
    pub maximum_temperature: Option<f64>,
    #[serde(default)]
    pub production_type: FluidIOMode,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FluidEnergySource {
    pub effectivity: f64,
    pub fluid_usage_per_tick: f64,
    pub scale_fluid_usage: bool,
    pub maximum_temperature: f64,
    pub burns_fluid: bool,
    pub emissions_per_minute: Option<Dict<f64>>,
    pub fluid_box: FluidBox,
}
impl Default for FluidEnergySource {
    fn default() -> Self {
        FluidEnergySource {
            effectivity: 1.0,
            fluid_usage_per_tick: 0.0,
            scale_fluid_usage: false,
            maximum_temperature: 0.0,
            burns_fluid: false,
            emissions_per_minute: None,
            fluid_box: FluidBox {
                filter: None,
                minimum_temperature: None,
                maximum_temperature: None,
                production_type: FluidIOMode::None,
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct VoidEnergySource {
    pub emissions_per_minute: Option<Dict<f64>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EffectReceiver {
    pub base_effect: Effect,
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

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Effect {
    pub consumption: f64,
    pub speed: f64,
    pub productivity: f64,
    pub pollution: f64,
    pub quality: f64,
}

impl Add for Effect {
    type Output = Effect;
    fn add(self, rhs: Self) -> Self::Output {
        Effect {
            consumption: self.consumption + rhs.consumption,
            speed: self.speed + rhs.speed,
            productivity: self.productivity + rhs.productivity,
            pollution: self.pollution + rhs.pollution,
            quality: self.quality + rhs.quality,
        }
    }
}

impl Effect {
    pub fn clamped(&self) -> Effect {
        Effect {
            consumption: self.consumption.clamp(-0.8, 327.67),
            speed: self.speed.clamp(-0.8, 327.67),
            productivity: self.productivity.clamp(0.0, 327.67),
            pollution: self.pollution.clamp(-0.8, 327.67),
            quality: self.quality.clamp(0.0, 327.67),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum EffectTypeLimitation {
    Single(String),
    Multiple(Vec<String>),
    Empty(HashMap<String, Value>),
}

impl Default for EffectTypeLimitation {
    fn default() -> Self {
        EffectTypeLimitation::Multiple(vec![])
    }
}

#[test]
fn test_energy_amount_deserialize() {
    let ea1: EnergyAmount = serde_json::from_str(r#""150kJ""#).unwrap();
    assert_eq!(ea1.amount as i32, 150_000.0 as i32);
    let ea2: EnergyAmount = serde_json::from_str(r#""2.5MW""#).unwrap();
    assert_eq!((ea2.amount * 60.0) as i32, 2_500_000.0 as i32);
    println!("{}", EnergyAmount { amount: 150000.0 });
}

#[derive(Debug, Clone, Deserialize)]
/// 子组
pub struct ItemSubgroup {
    #[serde(flatten)]
    pub base: PrototypeBase,
    /// 所属组
    pub group: String,
}

impl HasPrototypeBase for ItemSubgroup {
    fn base(&self) -> &PrototypeBase {
        &self.base
    }
}

pub fn get_order_info<T: HasPrototypeBase + Clone>(
    vec: &HashMap<String, T>,
    groups: &Dict<PrototypeBase>,
    subgroups: &Dict<ItemSubgroup>,
) -> OrderInfo {
    let mut grouped: HashMap<&String, HashMap<&String, Vec<&T>>> = HashMap::new();
    let other = &"other".to_string();
    let empty = &"".to_string();
    for prototype in vec.values() {
        let subgroup_name = &prototype.base().subgroup;
        if let Some(subgroup) = subgroups.get(subgroup_name) {
            let group_name = &subgroup.group;
            if let Some(group) = groups.get(group_name) {
                let group_entry = grouped.entry(&group.base().name).or_default();
                let subgroup_entry = group_entry.entry(&subgroup.base.name).or_default();
                subgroup_entry.push(prototype);
            } else {
                let group_entry = grouped.entry(other).or_default();
                let subgroup_entry = group_entry.entry(&subgroup.base.name).or_default();
                subgroup_entry.push(prototype);
            }
        } else {
            let group_entry = grouped.entry(other).or_default();
            let subgroup_entry = group_entry.entry(empty).or_default();
            subgroup_entry.push(prototype);
        }
    }

    let mut ret = vec![];

    let mut group_keys: Vec<&&String> = grouped.keys().collect();
    // Use sort_by with borrowed keys instead of sort_by_key to avoid cloning
    group_keys.sort_by(|a, b| {
        let a_order = groups.get(**a).map(|g| &g.order);
        let b_order = groups.get(**b).map(|g| &g.order);
        a_order.cmp(&b_order)
    });

    for group_key in group_keys {
        let subgroups_map = grouped.get(group_key).unwrap();
        let mut subgroup_keys: Vec<&&String> = subgroups_map.keys().collect();
        // Use sort_by with borrowed keys instead of sort_by_key to avoid cloning
        subgroup_keys.sort_by(|a, b| {
            let a_order = subgroups.get(**a).map(|sg| &sg.base.order);
            let b_order = subgroups.get(**b).map(|sg| &sg.base.order);
            a_order.cmp(&b_order)
        });

        let mut subgroup_vec = vec![];
        for subgroup_key in subgroup_keys {
            let prototypes = subgroups_map.get(subgroup_key).unwrap();
            let mut sorted_prototypes = prototypes.clone();
            sorted_prototypes.sort_by_key(|p| (&p.base().order, &p.base().name));
            let prototype_names: Vec<String> = sorted_prototypes
                .iter()
                .map(|p| p.base().name.clone())
                .collect();
            subgroup_vec.push(((*subgroup_key).clone(), prototype_names));
        }
        ret.push(((*group_key).clone(), subgroup_vec));
    }

    ret
}

pub fn get_reverse_order_info(order_info: &OrderInfo) -> ReverseOrderInfo {
    let mut reverse_map: ReverseOrderInfo = HashMap::new();
    for (group_index, group) in order_info.iter().enumerate() {
        for (subgroup_index, subgroup) in group.1.iter().enumerate() {
            for (item_index, item_name) in subgroup.1.iter().enumerate() {
                reverse_map.insert(item_name.clone(), (group_index, subgroup_index, item_index));
            }
        }
    }
    reverse_map
}

/// Helper function to generate sort key for a GenericItem
/// Returns (category, order_info, name) tuple for sorting
fn get_generic_item_sort_key<'a>(
    item: &'a GenericItem,
    ctx: &FactorioContext,
) -> (usize, (usize, usize, usize), &'a str) {
    match item {
        GenericItem::Item { name, quality } => (
            *quality as usize,
            ctx.order_of_entries["item"].get(name).copied().unwrap_or((0, 0, 0)),
            "",
        ),
        GenericItem::Fluid {
            name,
            temperature: _,
        } => (
            0x100usize,
            ctx.order_of_entries["fluid"].get(name).copied().unwrap_or((0, 0, 0)),
            "",
        ),
        GenericItem::Entity { name, quality } => (
            0x200usize + *quality as usize,
            ctx.order_of_entries["entity"].get(name).copied().unwrap_or((0, 0, 0)),
            "",
        ),
        GenericItem::Heat => (0x300usize, (0usize, 0usize, 0usize), ""),
        GenericItem::Electricity => (0x400usize, (0usize, 0usize, 0usize), ""),
        GenericItem::FluidHeat { filter } => (
            0x500usize,
            (0usize, 0usize, 0usize),
            filter.as_deref().unwrap_or(""),
        ),
        GenericItem::FluidFuel { filter } => (
            0x600usize,
            (0usize, 0usize, 0usize),
            filter.as_deref().unwrap_or(""),
        ),
        GenericItem::ItemFuel { category } => {
            (0x700usize, (0usize, 0usize, 0usize), category.as_str())
        }
        GenericItem::RocketPayloadWeight => (0x800usize, (0usize, 0usize, 0usize), ""),
        GenericItem::RocketPayloadStack => (0x900usize, (0usize, 0usize, 0usize), ""),
        GenericItem::Pollution { name } => (0xa00usize, (0usize, 0usize, 0usize), name.as_str()),
        GenericItem::Custom { name } => (0xb00usize, (0usize, 0usize, 0usize), name.as_str()),
    }
}

pub fn sort_generic_items(keys: &mut Vec<&GenericItem>, ctx: &FactorioContext) {
    // Use sort_by instead of sort_by_key to avoid cloning strings during comparison
    keys.sort_by(|a, b| {
        let a_key = get_generic_item_sort_key(a, ctx);
        let b_key = get_generic_item_sort_key(b, ctx);
        a_key.cmp(&b_key)
    });
}

/// Sort a vector of owned GenericItems in-place
pub fn sort_generic_items_owned(keys: &mut Vec<GenericItem>, ctx: &FactorioContext) {
    keys.sort_by(|a, b| {
        let a_key = get_generic_item_sort_key(a, ctx);
        let b_key = get_generic_item_sort_key(b, ctx);
        a_key.cmp(&b_key)
    });
}
