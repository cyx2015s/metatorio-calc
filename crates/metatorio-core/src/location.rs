//! Universe-factory coordinates and mechanics.
//!
//! A normal factory has one implicit environment. A universe factory plans
//! several locations at once, so every flow and every mechanic must carry the
//! location it belongs to.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dual_var::DualVar;
use crate::id::IdWithQuality;
use crate::mechanic::Mechanic;

/// A physical planning location.
///
/// `Planet(name)` uses `name` both as the planet prototype and as its surface:
/// current Factorio data does not expose multiple surfaces per planet. `Space`
/// is the orbital/space-platform environment.
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

/// A flow coordinate qualified by its location.
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

/// A normal mechanic placed at one universe location.
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

/// Cargo transported between a planet and space.
///
/// `location` is the planet: the origin of RocketExport, or destination of OrbitalDrop.
/// Both directions preserve the item identity, including quality.
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

/// Local production and both explicit directions of orbital cargo transport.
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
