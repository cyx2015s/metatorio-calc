use serde::Deserialize;

use crate::ctx::factorio::common::{EnergyAmount, PrototypeBase};

pub(crate) const ITEM_TYPES: &[&str] = &[
    "item",
    "ammo",
    "capsule",
    "gun",
    "item-with-entity-data",
    "item-with-label",
    "item-with-inventory",
    "blueprint-book",
    "item-with-tags",
    "selection-tool",
    "blueprint",
    "copy-paste-tool",
    "deconstruction-item",
    "spidertron-remote",
    "upgrade-item",
    "module",
    "rail-planner",
    "space-platform-starter-pack",
    "tool",
    "armor",
    "repair-tool",
];

/// 仅存储物品的基础属性，插件属性另行收集
#[derive(Debug, Clone, Deserialize)]
struct ItemPrototype {
    #[serde(flatten)]
    base: PrototypeBase,

    /// 变质可以自然发生，不绑定任何机器，所以属性存储在 Item 里
    #[serde(flatten)]
    spoil: Option<SpoilProperty>,

    /// 燃烧作为能量来源，可以发生在多种机器中，所以属性存储在 Item 里
    #[serde(flatten)]
    burn: Option<BurnProperty>,

    /// 种植实际上绑定农业塔，但完整的循环包括种子、植株、产物 3 个物品
    /// 另外所有物品都可以用作种子，没有单独的原型来区分，所以放这里最合适
    /// 农业塔不区分种子，种子也没有放置条件，是对应的植株有生长条件
    /// 所有考虑种植机制时，将植株本身存储为类配方，农业塔视作机器
    #[serde(flatten)]
    plant: Option<PlantProperty>,

    /// Tile
    place_as_tile: Option<PlaceAsTileProperty>,

    /// Entity
    place_result: Option<String>,
}

impl Default for ItemPrototype {
    fn default() -> Self {
        ItemPrototype {
            base: PrototypeBase {
                r#type: "item".to_string(),
                name: "item-unknown".to_string(),
                ..Default::default()
            },
            spoil: None,
            burn: None,
            plant: None,
            place_as_tile: None,
            place_result: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct SpoilProperty {
    spoil_ticks: f64,
    spoil_result: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct BurnProperty {
    fuel_value: EnergyAmount,
    burnt_result: Option<String>,
    fuel_category: Option<String>,
    fuel_emissions_multiplier: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
struct PlantProperty {
    plant_result: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct PlaceAsTileProperty {
    result: String,
}