use std::{collections::HashMap, fmt::Display};

use serde::{Deserialize, Deserializer, de::DeserializeOwned};
use serde_json::{Value, from_value};

pub(crate) type Dict<T> = HashMap<String, T>;
pub(crate) type Emissions = Dict<f64>;
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum MapPosition {
    Struct { x: f64, y: f64 },
    Tuple(f64, f64),
}
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum BoundingBox {
    Struct {
        left_top: MapPosition,
        right_bottom: MapPosition,
        orientation: Option<f64>,
    },
    Pair(MapPosition, MapPosition),
    Triplet(MapPosition, MapPosition, f64),
}

pub(crate) fn as_vec_or_empty<'de, T, D>(deserializer: D) -> Result<Vec<T>, D::Error>
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

pub(crate) fn option_as_vec_or_empty<'de, T, D>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
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
pub(crate) struct PrototypeBase {
    /// 类型名
    pub(crate) r#type: String,
    /// 内部名
    pub(crate) name: String,
    /// 排序依据
    pub(crate) order: String,
    /// 子组
    pub(crate) subgroup: String,
    /// 默认隐藏
    pub(crate) hidden: bool,
    /// 视为参数
    pub(crate) parameter: bool,
}

impl Default for PrototypeBase {
    fn default() -> Self {
        PrototypeBase {
            r#type: String::new(),
            name: String::new(),
            order: String::new(),
            subgroup: String::new(),
            hidden: false,
            parameter: false,
        }
    }
}

#[derive(Debug, Clone)]
/// 能量数量，单位为焦耳（J），如果是功率则为焦耳每刻（J/tick）
pub(crate) struct EnergyAmount {
    amount: f64,
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
            match dimension_char {
                Some('W') => multiplier /= 60.0,
                _ => {}
            }
            let numeric_value: f64 = value
                .trim_end_matches(|c: char| !c.is_digit(10))
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

pub(crate) enum EnergySource {
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
pub(crate) struct ElectricEnergySource {
    buffer_capacity: Option<EnergyAmount>,
    input_flow_limit: Option<EnergyAmount>,
    output_flow_limit: Option<EnergyAmount>,
    drain: Option<EnergyAmount>,
    emissions_per_minute: Option<Emissions>,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct BurnerEnergySource {
    burnt_inventory_size: f64,
    effectivity: f64,
    burner_usage: String,
    emissions_per_minute: Option<HashMap<String, f64>>,
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
pub(crate) struct HeatEnergySource {
    max_temperature: f64,
    emissions_per_minute: Option<HashMap<String, f64>>,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub(crate) struct VoidEnergySource {
    emissions_per_minute: Option<HashMap<String, f64>>,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub(crate) struct Effect {
    consumption: f64,
    speed: f64,
    productivity: f64,
    efficiency: f64,
    quality: f64,
}

#[derive(Debug, Clone, Deserialize)]
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

#[test]
fn test_energy_amount_deserialize() {
    let ea1: EnergyAmount = serde_json::from_str(r#""150kJ""#).unwrap();
    assert_eq!(ea1.amount as i32, 150_000.0 as i32);
    let ea2: EnergyAmount = serde_json::from_str(r#""2.5MW""#).unwrap();
    assert_eq!((ea2.amount * 60.0) as i32, 2_500_000.0 as i32);
    println!("{}", EnergyAmount { amount: 150000.0 });
}
