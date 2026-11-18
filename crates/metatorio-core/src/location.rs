//! 宇宙工厂的空间坐标与机制包装。
//!
//! 普通工厂只有单一的隐式环境；宇宙工厂同时规划多个地点，
//! 因此每条流和每个机制都必须显式携带所属位置。

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dual_var::DualVar;
use crate::id::IdWithQuality;
use crate::mechanic::Mechanic;

/// 物理规划位置。
///
/// `Planet(name)` 同时作为行星原型名与地表名（当前游戏数据不支持同星球多地表）；
/// `Space` 表示轨道/太空平台环境。
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(tag = "kind", content = "name", rename_all = "kebab-case")]
pub enum Location {
    Planet(String),
    Space,
}

impl Location {
    pub fn planet(name: impl Into<String>) -> Self {
        Self::Planet(name.into())
    }

    pub fn space() -> Self {
        Self::Space
    }

    pub fn is_space(&self) -> bool {
        matches!(self, Self::Space)
    }

    pub fn planet_name(&self) -> Option<&str> {
        match self {
            Self::Planet(name) => Some(name),
            Self::Space => None,
        }
    }
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Planet(name) => write!(f, "{name}"),
            Self::Space => f.write_str("space"),
        }
    }
}

/// 附带空间位置的流坐标。
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct LocatedDualVar {
    pub location: Location,
    pub flow: DualVar,
}

impl Default for LocatedDualVar {
    fn default() -> Self {
        Self {
            location: Location::Space,
            flow: DualVar::Unknown,
        }
    }
}

impl LocatedDualVar {
    pub fn new(location: Location, flow: DualVar) -> Self {
        Self { location, flow }
    }
}

/// 放置于某一宇宙位置的普通机制。
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalLocatedMechanic {
    pub location: Location,
    pub mechanic: Mechanic,
}

impl Default for LocalLocatedMechanic {
    fn default() -> Self {
        Self {
            location: Location::Space,
            mechanic: Mechanic::Recipe(Default::default()),
        }
    }
}

/// 行星与太空之间的货运机制。
///
/// `location` 为所属行星：`RocketExport` 的起点，或 `OrbitalDrop` 的终点；
/// 两个方向均保留包含品质在内的完整物品身份。
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OrbitalCargoMechanic {
    pub location: Location,
    pub item: IdWithQuality,
}

impl Default for OrbitalCargoMechanic {
    fn default() -> Self {
        Self {
            location: Location::Space,
            item: IdWithQuality::default(),
        }
    }
}

/// 本地生产与双向星际货运机制的统一枚举。
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum LocatedMechanic {
    Local(LocalLocatedMechanic),
    RocketExport(OrbitalCargoMechanic),
    OrbitalDrop(OrbitalCargoMechanic),
}

impl Default for LocatedMechanic {
    fn default() -> Self {
        Self::Local(LocalLocatedMechanic::default())
    }
}

impl LocatedMechanic {
    pub fn location(&self) -> &Location {
        match self {
            Self::Local(mechanic) => &mechanic.location,
            Self::RocketExport(mechanic) | Self::OrbitalDrop(mechanic) => &mechanic.location,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_json_is_strongly_tagged() {
        assert_eq!(
            serde_json::to_value(Location::planet("nauvis")).unwrap(),
            serde_json::json!({"kind": "planet", "name": "nauvis"})
        );
        assert_eq!(
            serde_json::to_value(Location::Space).unwrap(),
            serde_json::json!({"kind": "space"})
        );
    }
}
