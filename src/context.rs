use std::collections::HashMap;

use crate::types::{
    CraftingMachinePrototype, FluidPrototype, ItemPrototype, LabPrototype, MiningDrillPrototype,
    RecipePrototype, ResourceEntityPrototype,
};
use serde_json::Value;
#[derive(Debug)]
pub(crate) struct FactoryContext {
    /// 组装机，熔炉，火箭发射井，和传统配方组合
    pub(crate) machines: HashMap<String, CraftingMachinePrototype>,

    /// 传统配方
    pub(crate) recipes: HashMap<String, RecipePrototype>,

    /// 研究中心
    pub(crate) labs: HashMap<String, LabPrototype>,
    /// 挖矿机
    pub(crate) drills: HashMap<String, MiningDrillPrototype>,

    /// 对应到线性规划中需要满足总产量非负的项
    /// 所有物品
    pub(crate) items: HashMap<String, ItemPrototype>,
    /// 所有流体
    pub(crate) fluids: HashMap<String, FluidPrototype>,
    /// 资源实体
    pub(crate) resource_entities: HashMap<String, ResourceEntityPrototype>,
}

impl FactoryContext {
    pub(crate) fn load_from_json(data_raw_dump: &Value) -> Self {
        let mut machines = HashMap::new();
        let mut recipes = HashMap::new();
        let mut items = HashMap::new();
        let mut fluids = HashMap::new();
        let mut resource_entities = HashMap::new();
        let mut labs = HashMap::new();
        let mut drills = HashMap::new();
        {
            let item_types = vec![
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
            for item_type in item_types {
                if let Some(item_type_values) = data_raw_dump[item_type].as_object() {
                    for (name, value) in item_type_values.iter() {
                        let item: ItemPrototype = serde_json::from_value(value.clone()).unwrap();
                        items.insert(name.clone(), item);
                    }
                }
            }
        }
        {
            for (name, value) in data_raw_dump["recipe"].as_object().unwrap().iter() {
                let recipe: RecipePrototype = serde_json::from_value(value.clone()).unwrap();
                recipes.insert(name.clone(), recipe);
            }
        }
        {
            let machine_types = vec!["furnace", "assembling-machine", "rocket-silo"];
            for machine_type in machine_types {
                if let Some(machine_type_values) = data_raw_dump[machine_type].as_object() {
                    for (name, value) in machine_type_values.iter() {
                        let machine: CraftingMachinePrototype =
                            serde_json::from_value(value.clone()).unwrap();
                        machines.insert(name.clone(), machine);
                    }
                }
            }
        }
        {
            let fluid_types = vec!["fluid"];
            for fluid_type in fluid_types {
                if let Some(fluid_type_values) = data_raw_dump[fluid_type].as_object() {
                    for (name, value) in fluid_type_values.iter() {
                        let fluid: FluidPrototype = serde_json::from_value(value.clone()).unwrap();
                        fluids.insert(name.clone(), fluid);
                    }
                }
            }
        }
        {
            let entity_resource_types = vec![
                "accumulator",
                "agricultural-tower",
                "artillery-turret",
                "asteroid-collector",
                "asteroid",
                "beacon",
                "boiler",
                "burner-generator",
                "cargo-bay",
                "cargo-landing-pad",
                "cargo-pod",
                "character",
                "arithmetic-combinator",
                "decider-combinator",
                "selector-combinator",
                "constant-combinator",
                "container",
                "logistic-container",
                "infinity-container",
                "temporary-container",
                "assembling-machine",
                "rocket-silo",
                "furnace",
                "display-panel",
                "electric-energy-interface",
                "electric-pole",
                "unit-spawner",
                "capture-robot",
                "combat-robot",
                "construction-robot",
                "logistic-robot",
                "fusion-generator",
                "fusion-reactor",
                "gate",
                "generator",
                "heat-interface",
                "heat-pipe",
                "inserter",
                "lab",
                "lamp",
                "land-mine",
                "lightning-attractor",
                "linked-container",
                "market",
                "mining-drill",
                "offshore-pump",
                "pipe",
                "infinity-pipe",
                "pipe-to-ground",
                "player-port",
                "power-switch",
                "programmable-speaker",
                "proxy-container",
                "pump",
                "radar",
                "curved-rail-a",
                "elevated-curved-rail-a",
                "curved-rail-b",
                "elevated-curved-rail-b",
                "half-diagonal-rail",
                "elevated-half-diagonal-rail",
                "legacy-curved-rail",
                "legacy-straight-rail",
                "rail-ramp",
                "straight-rail",
                "elevated-straight-rail",
                "rail-chain-signal",
                "rail-signal",
                "rail-support",
                "reactor",
                "roboport",
                "segment",
                "segmented-unit",
                "simple-entity-with-owner",
                "simple-entity-with-force",
                "solar-panel",
                "space-platform-hub",
                "spider-leg",
                "spider-unit",
                "storage-tank",
                "thruster",
                "train-stop",
                "lane-splitter",
                "linked-belt",
                "loader-1x1",
                "loader",
                "splitter",
                "transport-belt",
                "underground-belt",
                "turret",
                "ammo-turret",
                "electric-turret",
                "fluid-turret",
                "unit",
                "valve",
                "car",
                "artillery-wagon",
                "cargo-wagon",
                "infinity-cargo-wagon",
                "fluid-wagon",
                "locomotive",
                "spider-vehicle",
                "wall",
                "fish",
                "simple-entity",
                "tree",
                "plant",
                "resource",
                "rocket-silo-rocket",
            ];
            for entity_resource_type in entity_resource_types {
                if let Some(entity_resource_type_values) =
                    data_raw_dump[entity_resource_type].as_object()
                {
                    for (name, value) in entity_resource_type_values.iter() {
                        let entity_resource: ResourceEntityPrototype =
                            serde_json::from_value(value.clone()).unwrap();
                        if entity_resource.minable.is_some() || entity_resource.loot.is_some() {
                            // 只筛选有效的资源实体
                            resource_entities.insert(name.clone(), entity_resource);
                        }
                    }
                }
            }
        }
        {
            for (name, value) in data_raw_dump["lab"].as_object().unwrap().iter() {
                let lab: LabPrototype = serde_json::from_value(value.clone()).unwrap();
                labs.insert(name.clone(), lab);
            }
        }
        {
            let misc_drill_types = vec![
                "mining-drill",
                "asteroid-collector",
                "agricultural-tower",
                "offshore-pump",
                "boiler",
                "burner-generator",
            ];
            for misc_drill_type in misc_drill_types {
                if let Some(misc_drill_type_values) = data_raw_dump[misc_drill_type].as_object() {
                    for (name, value) in misc_drill_type_values.iter() {
                        let drill: MiningDrillPrototype =
                            serde_json::from_value(value.clone()).unwrap();
                        drills.insert(name.clone(), drill);
                    }
                }
            }
        }
        FactoryContext {
            machines,
            recipes,
            items,
            fluids,
            resource_entities,
            labs,
            drills,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs::read_to_string, path::Path};

    use serde_json::from_str;

    use super::*;
    #[test]
    fn test_load_context() {
        dotenv::dotenv().ok();
        let path = Path::new(std::env::var("FACTORIO_PATH").unwrap().as_str())
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("script-output/data-raw-dump__.json");
        // println!("{}", path.to_str().unwrap());
        let data_raw_dump: Value = from_str(read_to_string(path).unwrap().as_str()).unwrap();
        let context = FactoryContext::load_from_json(&data_raw_dump);
        println!(
            "Loaded context: {} machines, {} recipes, {} items, {} fluids, {} resource entities",
            context.machines.len(),
            context.recipes.len(),
            context.items.len(),
            context.fluids.len(),
            context.resource_entities.len()
        );

        println!("{:#?}", context);
    }
}
