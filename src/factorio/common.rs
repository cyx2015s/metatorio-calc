use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    hash::Hash,
    ops::{Add, Mul},
};

use indexmap::IndexMap;
use serde_json::Value;

use crate::{concept::*, factorio::*};


#[derive(Debug, Clone, Default)]
pub struct FactorioContext {
    pub data: DataContext,
    pub user: UserContext,
}

pub type Dict<T> = HashMap<String, T>;
pub type Emissions = Dict<f64>;
pub type OrderInfo = Vec<(String, Vec<(String, Vec<String>)>)>;
pub type ReverseOrderInfo = HashMap<String, (usize, usize, usize)>;
pub type AsFactorioFlow = dyn AsFlow<Game = FactorioContext, Item = GenericItem>;
pub type FactorioMechanic = dyn Mechanic<FactorioContext, GenericItem>;
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct IdWithQuality(pub String, pub u8);

impl From<&IdWithQuality> for IdWithQuality {
    fn from(id: &IdWithQuality) -> Self {
        IdWithQuality(id.0.clone(), id.1)
    }
}

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

impl TryFrom<&GenericItem> for IdWithQuality {
    type Error = &'static str;
    fn try_from(value: &GenericItem) -> Result<Self, Self::Error> {
        match value {
            GenericItem::Item(IdWithQuality(name, quality)) => {
                Ok(IdWithQuality(name.clone(), *quality))
            }
            GenericItem::Entity(IdWithQuality(name, quality)) => {
                Ok(IdWithQuality(name.clone(), *quality))
            }
            _ => Err("无法从非物品类型的 GenericItem 转换为 IdWithQuality"),
        }
    }
}

impl TryFrom<GenericItem> for IdWithQuality {
    type Error = &'static str;
    fn try_from(value: GenericItem) -> Result<Self, Self::Error> {
        match value {
            GenericItem::Item(IdWithQuality(name, quality)) => Ok(IdWithQuality(name, quality)),
            GenericItem::Entity(IdWithQuality(name, quality)) => Ok(IdWithQuality(name, quality)),
            _ => Err("无法从非物品类型的 GenericItem 转换为 IdWithQuality"),
        }
    }
}

pub fn index_map_update_entry<T, N>(map: &mut IndexMap<T, N>, key: T, value: N)
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

impl From<Color> for egui::Color32 {
    fn from(val: Color) -> Self {
        egui::Color32::from_rgba_unmultiplied(val.0, val.1, val.2, val.3)
    }
}

impl<'de> serde::Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value: Value = serde::Deserialize::deserialize(deserializer)?;
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

impl<'de> serde::Deserialize<'de> for MapPosition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value: Value = serde::Deserialize::deserialize(deserializer)?;
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

#[derive(Debug, Clone, serde::Deserialize)]
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

impl BoundingBox {
    pub fn get_area(&self) -> f64 {
        match self {
            BoundingBox::Struct {
                left_top,
                right_bottom,
                ..
            } => {
                let width = right_bottom.0 - left_top.0;
                let height = right_bottom.1 - left_top.1;
                width.ceil() * height.ceil()
            }
            BoundingBox::Pair(left_top, right_bottom) => {
                let width = right_bottom.0 - left_top.0;
                let height = right_bottom.1 - left_top.1;
                width.ceil() * height.ceil()
            }
            BoundingBox::Triplet(left_top, right_bottom, _) => {
                let width = right_bottom.0 - left_top.0;
                let height = right_bottom.1 - left_top.1;
                width.ceil() * height.ceil()
            }
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
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

impl<'de> serde::Deserialize<'de> for EnergyAmount {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let value: String = serde::Deserialize::deserialize(deserializer)?;
        if let Some(amount) = parse_energy(&value) {
            Ok(EnergyAmount { amount })
        } else {
            Err(serde::de::Error::custom(format!(
                "不是有效的能量字符串: {}",
                &value
            )))
        }
    }
}

impl Display for EnergyAmount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}J", compact_number(self.amount))
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum EnergySource {
    Electric(ElectricEnergySource),
    Burner(BurnerEnergySource),
    Heat(HeatEnergySource),
    Fluid(FluidEnergySource),
    Void(VoidEnergySource),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct ElectricEnergySource {
    buffer_capacity: Option<EnergyAmount>,
    input_flow_limit: Option<EnergyAmount>,
    output_flow_limit: Option<EnergyAmount>,
    pub drain: Option<EnergyAmount>,
    pub emissions_per_minute: Option<Emissions>,
}

#[derive(Debug, Clone, serde::Deserialize)]
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
            burner_usage: "chemical".to_string(),
            emissions_per_minute: None,
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(default)]
pub struct HeatEnergySource {
    pub max_temperature: f64,
    pub emissions_per_minute: Option<Dict<f64>>,
}

#[derive(Debug, serde::Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FluidIOMode {
    #[default]
    None,
    Input,
    InputOutput,
    Output,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FluidBox {
    #[serde(default)]
    pub filter: Option<String>,
    pub minimum_temperature: Option<f64>,
    pub maximum_temperature: Option<f64>,
    #[serde(default)]
    pub production_type: FluidIOMode,
}

#[derive(Debug, Clone, serde::Deserialize)]
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

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(default)]
pub struct VoidEnergySource {
    pub emissions_per_minute: Option<Dict<f64>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub struct EffectReceiver {
    pub base_effect: Effect,
    pub use_module_effects: bool,
    pub use_beacon_effects: bool,
    pub use_surface_effects: bool,
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
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

impl Mul<f64> for Effect {
    type Output = Effect;
    fn mul(self, rhs: f64) -> Self::Output {
        Effect {
            consumption: self.consumption * rhs,
            speed: self.speed * rhs,
            productivity: self.productivity * rhs,
            pollution: self.pollution * rhs,
            quality: self.quality * rhs,
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

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectType {
    Consumption,
    Speed,
    Productivity,
    Pollution,
    Quality,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum EffectTypeLimitation {
    Single(EffectType),
    Multiple(Vec<EffectType>),
    Empty(Value),
}

impl Default for EffectTypeLimitation {
    fn default() -> Self {
        EffectTypeLimitation::Multiple(vec![])
    }
}

impl EffectTypeLimitation {
    pub fn new(
        allow_consumption: bool,
        allow_speed: bool,
        allow_productivity: bool,
        allow_pollution: bool,
        allow_quality: bool,
    ) -> Self {
        let mut ret = vec![];
        if allow_consumption {
            ret.push(EffectType::Consumption);
        }
        if allow_speed {
            ret.push(EffectType::Speed);
        }
        if allow_productivity {
            ret.push(EffectType::Productivity);
        }
        if allow_pollution {
            ret.push(EffectType::Pollution);
        }
        if allow_quality {
            ret.push(EffectType::Quality);
        }
        EffectTypeLimitation::Multiple(ret)
    }

    pub fn normalized(&self) -> Self {
        match self {
            EffectTypeLimitation::Single(s) => EffectTypeLimitation::Multiple(vec![s.clone()]),
            EffectTypeLimitation::Empty(_) => EffectTypeLimitation::default(),
            other => other.clone(),
        }
    }

    pub fn intersect(&self, other: &EffectTypeLimitation) -> EffectTypeLimitation {
        let self_normalized = self.normalized();
        let other_normalized = other.normalized();
        match (self_normalized, other_normalized) {
            (EffectTypeLimitation::Multiple(v1), EffectTypeLimitation::Multiple(v2)) => {
                let intersection: Vec<EffectType> =
                    v1.into_iter().filter(|item| v2.contains(item)).collect();
                EffectTypeLimitation::Multiple(intersection)
            }
            _ => EffectTypeLimitation::default(),
        }
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

#[derive(Debug, Clone, serde::Deserialize)]
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
    ctx: &'a FactorioContext,
) -> (usize, (usize, usize, usize), &'a str) {
    let data = &ctx.data;
    match item {
        GenericItem::Item(IdWithQuality(name, quality)) => (
            *quality as usize,
            data.order_of_entries["item"]
                .get(name)
                .copied()
                .unwrap_or((0, 0, 0)),
            "",
        ),
        GenericItem::Fluid {
            name,
            temperature: _,
        } => (
            0x100usize,
            data.order_of_entries["fluid"]
                .get(name)
                .copied()
                .unwrap_or((0, 0, 0)),
            "",
        ),
        GenericItem::Entity(IdWithQuality(name, quality)) => (
            0x200usize + *quality as usize,
            data.order_of_entries["entity"]
                .get(name)
                .copied()
                .unwrap_or((0, 0, 0)),
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

pub fn sort_generic_items(keys: &mut Vec<&GenericItem>, factorio: &FactorioContext) {
    // Use sort_by instead of sort_by_key to avoid cloning strings during comparison
    keys.sort_by(|a, b| {
        let a_key = get_generic_item_sort_key(a, factorio);
        let b_key = get_generic_item_sort_key(b, factorio);
        a_key.cmp(&b_key)
    });
}

/// Sort a vector of owned GenericItems in-place
pub fn sort_generic_items_owned(keys: &mut [GenericItem], factorio: &FactorioContext) {
    keys.sort_by(|a, b| {
        let a_key = get_generic_item_sort_key(a, factorio);
        let b_key = get_generic_item_sort_key(b, factorio);
        a_key.cmp(&b_key)
    });
}
