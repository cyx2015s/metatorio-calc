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

use serde::de::{Deserializer, Error as _};
use serde_json::Value;

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
        let to_u8 = |v: &Value| -> Option<u8> {
            v.as_f64().map(|f| (f * 255.0).round() as u8)
        };
        match value {
            Value::Array(vec) => {
                // mod 数据可能给空数组/短数组（Lua 空 table 导出 {} 的另一种形态）——通道补 0
                let mut c = Color(0, 0, 0, 255);
                if vec.len() >= 1 { c.0 = to_u8(&vec[0]).unwrap_or(0); }
                if vec.len() >= 2 { c.1 = to_u8(&vec[1]).unwrap_or(0); }
                if vec.len() >= 3 { c.2 = to_u8(&vec[2]).unwrap_or(0); }
                if vec.len() >= 4 { c.3 = to_u8(&vec[3]).unwrap_or(255); }
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
                let x = vec[0].as_f64().ok_or_else(|| D::Error::custom("MapPosition 数组首元素类型错误"))?;
                let y = vec[1].as_f64().ok_or_else(|| D::Error::custom("MapPosition 数组第二元素类型错误"))?;
                Ok(MapPosition(x, y))
            }
            _ => Err(D::Error::custom("MapPosition 不是对象或长度 ≥2 的数组")),
        }
    }
}


// ── 效果类型（EffectTypeLimitation）──────────────────────────────

/// 模块/信标效果类型（schema 的 union 成员是固定的 5 个字面值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectType {
    Speed,
    Productivity,
    Consumption,
    Pollution,
    Quality,
}

impl EffectType {
    pub fn parse(s: &str) -> Option<EffectType> {
        match s {
            "speed" => Some(EffectType::Speed),
            "productivity" => Some(EffectType::Productivity),
            "consumption" => Some(EffectType::Consumption),
            "pollution" => Some(EffectType::Pollution),
            "quality" => Some(EffectType::Quality),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            EffectType::Speed => "speed",
            EffectType::Productivity => "productivity",
            EffectType::Consumption => "consumption",
            EffectType::Pollution => "pollution",
            EffectType::Quality => "quality",
        }
    }
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
                let t = EffectType::parse(&s).ok_or_else(|| {
                    D::Error::custom(format!("未知效果类型: {s}"))
                })?;
                allowed.push(t);
            }
            Value::Array(items) => {
                for item in items {
                    if let Value::String(s) = item {
                        if let Some(t) = EffectType::parse(&s) {
                            allowed.push(t);
                        }
                        // 非字符串元素：容错跳过（mod 数据不规范）
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
            seq.serialize_element(t.name())?;
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
    fn effect_type_limitation_parses() {
        // 数组形态（dump 最常见）
        let e: EffectTypeLimitation = serde_json::from_str(r#"["speed", "consumption", "pollution"]"#).unwrap();
        assert_eq!(
            e.allowed,
            vec![EffectType::Speed, EffectType::Consumption, EffectType::Pollution]
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
}
