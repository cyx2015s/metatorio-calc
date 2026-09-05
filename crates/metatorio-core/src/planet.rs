//! 星球/地表相关的数据查询（从 metatorio-runtime 迁移到 core，供可达性等
//! 核心逻辑直接使用）：
//! - 当前工厂生效的表面属性（显式地表 > 同名关联地表 > 星球自身）
//! - 表面条件满足判定（配方/机器 surface_conditions，回退 surface-property 默认值）
//! - 星球自动生成的 tile 集合（map_gen_settings）
//! - 星球自动生成的可用流（autoplaced 资源 + 带流体 tile）——严格供给下也免费
//! - 种植物要求的地表（tile_buildability_rules.required_tiles 碰撞层 ⊆ tile 碰撞层）

use std::collections::{BTreeMap, HashSet};

use metatorio_data::store::{PrototypeGroup, PrototypeRecord, PrototypeStore};
use metatorio_data::{
    EntityComponent, FluidComponent, ItemComponent, PlanetComponent, PlanetPrototypeMapGenSettings,
    SurfaceComponent, SurfaceCondition, SurfacePropertyComponent, TileComponent,
};

use crate::dual_var::DualVar;
use crate::id::{IdWithQuality, NORMAL_QUALITY};
use crate::prim_var::Flow;

/// 当前工厂生效的表面属性：
/// 1. 显式设置地表 → 该地表的 surface_properties；
/// 2. 否则星球存在同名关联地表 → 关联地表的 surface_properties；
/// 3. 否则星球自身（太空等无关联地表的场景）→ 星球组件的 surface_properties；
/// 4. 都没有 → None（不做表面条件过滤）。
pub fn surface_properties_of(
    store: &PrototypeStore,
    planet: Option<&str>,
    surface: Option<&str>,
) -> Option<BTreeMap<String, f64>> {
    if let Some(surface_name) = surface {
        return store
            .get(PrototypeGroup::Surface, surface_name)
            .and_then(|record| record.component::<SurfaceComponent>())
            .map(|surface| surface.surface_properties.clone());
    }
    let planet_name = planet?;
    // 同名关联地表优先（星球与其关联地表是不同原型，属性可能不同）
    if let Some(surface) = store
        .get(PrototypeGroup::Surface, planet_name)
        .and_then(|record| record.component::<SurfaceComponent>())
    {
        return Some(surface.surface_properties.clone());
    }
    if let Some(planet) = store
        .get(PrototypeGroup::Planet, planet_name)
        .and_then(|record| record.component::<PlanetComponent>())
    {
        return Some(planet.surface_properties.clone());
    }
    // 太空地点（space-location）没有 surface_properties → None（不限制）
    None
}

/// 表面条件是否满足：属性缺失时回退到 surface-property 原型的 default_value
/// （再回退 0.0），min/max 越界即不满足。
pub fn surface_condition_satisfied(
    store: &PrototypeStore,
    conditions: &[SurfaceCondition],
    properties: &BTreeMap<String, f64>,
) -> bool {
    for condition in conditions {
        let value = properties
            .get(&condition.property)
            .copied()
            .or_else(|| {
                store
                    .get(PrototypeGroup::SurfaceProperty, &condition.property)
                    .and_then(|record| record.component::<SurfacePropertyComponent>())
                    .map(|component| component.default_value)
            })
            .unwrap_or(0.0);
        if let Some(min) = condition.min
            && value < min {
                return false;
            }
        if let Some(max) = condition.max
            && value > max {
                return false;
            }
    }
    true
}

/// 星球的 map_gen_settings（含自动生成规则）。
pub(crate) fn planet_map_gen(
    store: &PrototypeStore,
    planet: &str,
) -> Option<PlanetPrototypeMapGenSettings> {
    store
        .get(PrototypeGroup::Planet, planet)
        .and_then(|record| record.component::<PlanetComponent>())
        .and_then(|planet| planet.map_gen_settings.clone())
}

/// 星球自动生成的 tile 名集合：
/// `autoplace_settings["tile"].settings` 的键 + `autoplace_controls` 命中的 tile。
pub fn planet_generated_tiles(store: &PrototypeStore, planet: &str) -> HashSet<String> {
    let mut tiles = HashSet::new();
    let Some(map_gen) = planet_map_gen(store, planet) else {
        return tiles;
    };
    if let Some(tile_settings) = map_gen.autoplace_settings.get("tile") {
        tiles.extend(tile_settings.settings.keys().cloned());
    }
    for record in store.group(PrototypeGroup::Tile) {
        let Some(tile) = record.component::<TileComponent>() else {
            continue;
        };
        let Some(autoplace) = &tile.autoplace else {
            continue;
        };
        if let Some(control) = &autoplace.control
            && map_gen.autoplace_controls.contains_key(control) {
                tiles.insert(record.name.clone());
            }
    }
    tiles
}

/// 星球自动生成的可用流（免费源）：autoplaced 资源实体（供给**实体**流，
/// 由采矿机制消耗并产出物品）+ 自动生成且带流体的 tile（水/岩浆等，
/// 按默认温度）。
///
/// 资源实体改为供给 `DualVar::Entity`（而非直接给 minable 产物物品）：
/// 这样自动规划会选中采矿机制（消耗实体、产出物品），而不是绕过采矿
/// 直接免费取物品。外部输入显式覆盖同名流时仍以外部输入为准。
pub fn planet_autoplaced_flows(store: &PrototypeStore, planet: &str) -> Flow {
    let mut flow = Flow::default();
    let Some(map_gen) = planet_map_gen(store, planet) else {
        return flow;
    };
    let entity_settings: HashSet<String> = map_gen
        .autoplace_settings
        .get("entity")
        .map(|settings| settings.settings.keys().cloned().collect())
        .unwrap_or_default();

    let autoplaced = |record: &PrototypeRecord| -> bool {
        let Some(entity) = record.component::<EntityComponent>() else {
            return false;
        };
        let Some(autoplace) = &entity.autoplace else {
            return false;
        };
        autoplace
            .control
            .as_ref()
            .is_some_and(|control| map_gen.autoplace_controls.contains_key(control))
            || entity_settings.contains(&record.name)
    };

    // 资源实体：只有 resource 类视为无限（树/岩石等自动放置实体即使可挖掘
    // 也不免费）；供给实体流（采矿机制消耗），须可挖掘才有效。
    for record in store.group(PrototypeGroup::Entity) {
        if record.type_ != "resource" {
            continue;
        }
        if !autoplaced(record) {
            continue;
        }
        let Some(minable) = record
            .component::<EntityComponent>()
            .and_then(|e| e.minable())
        else {
            continue;
        };
        if minable.mining_time <= 0.0 {
            continue;
        }
        let has_output = minable.result.is_some() || !minable.results.is_empty();
        if !has_output {
            continue;
        }
        *flow
            .entry(DualVar::Entity(IdWithQuality::new(
                &record.name,
                NORMAL_QUALITY,
            )))
            .or_insert(0.0) += 1.0;
    }
    // 带流体的自动生成 tile → 流体流（默认温度）
    for tile_name in planet_generated_tiles(store, planet) {
        let Some(record) = store.get(PrototypeGroup::Tile, &tile_name) else {
            continue;
        };
        let Some(fluid_name) = record
            .component::<TileComponent>()
            .and_then(|tile| tile.fluid.clone())
        else {
            continue;
        };
        let temperature = fluid_record_temperature(store, &fluid_name);
        *flow
            .entry(DualVar::Fluid {
                name: fluid_name,
                temperature,
            })
            .or_insert(0.0) += 1.0;
    }
    flow
}

fn fluid_record_temperature(store: &PrototypeStore, name: &str) -> [i32; 2] {
    let temperature = store
        .get(PrototypeGroup::Fluid, name)
        .and_then(|record| record.component::<FluidComponent>())
        .map(|fluid| fluid.default_temperature as i32)
        .unwrap_or(0);
    [temperature, temperature]
}

/// 种植物（plant_result 实体）要求的地表：`tile_buildability_rules.required_tiles`
/// 的碰撞层 ⊆ tile.collision_mask.layers 的 tile 集合。
/// 无 buildability 限制返回 `None`（任意地表可种）。
pub fn plant_required_tiles(store: &PrototypeStore, plant_entity: &str) -> Option<HashSet<String>> {
    let component = store
        .get(PrototypeGroup::Entity, plant_entity)?
        .component::<EntityComponent>()?;
    let mut required: HashSet<String> = HashSet::new();
    for rule in &component.tile_buildability_rules {
        if let Some(mask) = &rule.required_tiles {
            required.extend(mask.layers.keys().cloned());
        }
    }
    if required.is_empty() {
        return None;
    }
    let mut usable = HashSet::new();
    for record in store.group(PrototypeGroup::Tile) {
        let Some(tile) = record.component::<TileComponent>() else {
            continue;
        };
        let tile_layers: HashSet<String> = tile.collision_mask.layers.keys().cloned().collect();
        if required.iter().all(|layer| tile_layers.contains(layer)) {
            usable.insert(record.name.clone());
        }
    }
    Some(usable)
}

/// 种子在该星球是否可用：
/// - 种植物有 buildability 限制 → 星球生成 tile ∩ 要求 tile ≠ ∅；
/// - 无限制 → 回退 `default_import_location` 匹配（planet 未设置时视为可用）。
pub fn seed_available_on_planet(
    store: &PrototypeStore,
    seed: &ItemComponent,
    plant_entity: &str,
    planet: Option<&str>,
) -> bool {
    if let Some(usable) = plant_required_tiles(store, plant_entity) {
        let Some(planet) = planet else {
            return true;
        };
        let generated = planet_generated_tiles(store, planet);
        if generated.is_empty() {
            return false;
        }
        return usable.iter().any(|tile| generated.contains(tile));
    }
    match planet {
        None => true,
        Some(planet) => seed.default_import_location == planet,
    }
}

/// 便捷：以 (planet, surface) 一次性取表面属性（供调用方过滤枚举）。
pub fn current_surface_properties(
    store: &PrototypeStore,
    planet: Option<&str>,
    surface: Option<&str>,
) -> Option<BTreeMap<String, f64>> {
    surface_properties_of(store, planet, surface)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_store() -> PrototypeStore {
        let dump = json!({
            "item": {
                "seed": {
                    "type": "item", "name": "seed",
                    "plant_result": "plant-entity",
                    "default_import_location": "nauvis"
                }
            },
            "entity": {
                "plant-entity": {
                    "type": "plant", "name": "plant-entity",
                    "tile_buildability_rules": [{
                        "required_tiles": { "layers": { "ground-tile": true } },
                        "colliding_tiles": { "layers": {} },
                        "remove_on_collision": false
                    }]
                },
                "rock": {
                    "type": "simple-entity", "name": "rock",
                    "autoplace": { "control": "rock" },
                    "minable": { "result": "stone" }
                }
            },
            "resource": {
                "iron-ore": {
                    "type": "resource", "name": "iron-ore",
                    "autoplace": { "control": "iron-ore" },
                    "minable": { "result": "iron-ore", "mining_time": 1.0 }
                }
            },
            "tile": {
                "grass": {
                    "type": "tile", "name": "grass",
                    "collision_mask": { "layers": { "ground-tile": true } }
                },
                "shallow-water": {
                    "type": "tile", "name": "shallow-water",
                    "collision_mask": { "layers": { "water-tile": true } },
                    "fluid": "water"
                }
            },
            "fluid": {
                "water": { "type": "fluid", "name": "water", "default_temperature": 15 }
            },
            "surface-property": {
                "gravity": { "type": "surface-property", "name": "gravity", "default_value": 10.0 }
            },
            "planet": {
                "nauvis": {
                    "type": "planet", "name": "nauvis",
                    "surface_properties": { "gravity": 10.0 },
                    "map_gen_settings": {
                        "autoplace_controls": { "iron-ore": {}, "rock": {} },
                        "autoplace_settings": {
                            "tile": { "settings": { "grass": {} }, "treat_missing_as_default": true }
                        }
                    }
                },
                "aquilo": {
                    "type": "planet", "name": "aquilo",
                    "surface_properties": { "gravity": 4.0 },
                    "map_gen_settings": {
                        "autoplace_controls": {},
                        "autoplace_settings": {
                            "tile": { "settings": { "shallow-water": {} }, "treat_missing_as_default": true }
                        }
                    }
                }
            },
            "surface": {
                "nauvis": { "type": "surface", "name": "nauvis", "surface_properties": { "gravity": 8.0 } }
            }
        });
        PrototypeStore::load(&dump).expect("dump 加载失败")
    }

    #[test]
    fn surface_properties_prefer_associated_surface() {
        let store = test_store();
        // 星球有同名关联地表（gravity 8.0）→ 用关联地表属性，而非星球自身（10.0）
        let props = surface_properties_of(&store, Some("nauvis"), None).unwrap();
        assert_eq!(props.get("gravity"), Some(&8.0));
        // 显式地表优先
        let props = surface_properties_of(&store, Some("nauvis"), Some("nauvis")).unwrap();
        assert_eq!(props.get("gravity"), Some(&8.0));
        // 无关联地表的星球（aquilo）→ 星球自身属性
        let props = surface_properties_of(&store, Some("aquilo"), None).unwrap();
        assert_eq!(props.get("gravity"), Some(&4.0));
        // 无星球 → None
        assert!(surface_properties_of(&store, None, None).is_none());
    }

    #[test]
    fn seed_available_by_generated_tile_intersection() {
        let store = test_store();
        let seed = store
            .get(PrototypeGroup::Item, "seed")
            .unwrap()
            .component::<ItemComponent>()
            .unwrap();
        // nauvis 生成 grass（ground-tile）→ 种子可种
        assert!(seed_available_on_planet(
            &store,
            seed,
            "plant-entity",
            Some("nauvis")
        ));
        // aquilo 只生成 shallow-water（water-tile）→ 不可种
        assert!(!seed_available_on_planet(
            &store,
            seed,
            "plant-entity",
            Some("aquilo")
        ));
        // 无星球 → 视为可用
        assert!(seed_available_on_planet(&store, seed, "plant-entity", None));
    }

    #[test]
    fn planet_autoplaced_flows_from_tiles() {
        let store = test_store();
        let flows = planet_autoplaced_flows(&store, "aquilo");
        // aquilo 生成 shallow-water → 水（默认温度）
        assert!(flows.contains_key(&DualVar::Fluid {
            name: "water".to_string(),
            temperature: [15, 15],
        }));
    }

    #[test]
    fn only_resource_entities_are_infinite_sources() {
        let store = test_store();
        let flows = planet_autoplaced_flows(&store, "nauvis");
        // resource 类（iron-ore）→ 免费**实体**流（由采矿机制消耗并产出物品）
        assert!(flows.contains_key(&DualVar::Entity(IdWithQuality::new("iron-ore", "normal"))));
        // 不应直接给物品流（否则绕过采矿机制）
        assert!(!flows.contains_key(&DualVar::Item(IdWithQuality::new("iron-ore", "normal"))));
        // 非 resource 的自动放置实体（rock，可挖掘）→ 不入免费列表
        assert!(!flows.contains_key(&DualVar::Entity(IdWithQuality::new("rock", "normal"))));
    }
}
