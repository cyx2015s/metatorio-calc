//! reducer 之前的原型名校验。
//!
//! reducer（[`crate::state::RuntimeState`]）不持有原型仓库，因此无法判断
//! `set-recipe: "not-a-real-recipe"` 这类输入是否合法——旧行为是**静默写入
//! 垃圾**并返回 `changed: true`。对 LLM agent 尤其危险：它会把幻觉出的名字
//! 当成成功。
//!
//! 这里在 [`crate::solve::Runtime::dispatch`] 进入 reducer 之前，用项目当前
//! 上下文校验消息里引用的原型名（配方 / 机器 / 资源 / 物品 / 流体 / 科技 /
//! 星球 / 品质）。**拿不到 store 时跳过**（项目还没绑定游戏数据，不应阻塞）。

use metatorio_core::{Fuel, IdWithQuality};
use metatorio_data::store::{PrototypeGroup, PrototypeStore};

use crate::document::AutoBeaconPlan;
use crate::message::{
    AppMessage, FactoryAction, FactoryContextAction, MechanicAction, MiningMechanicAction,
    ModuleAction, PlanningAction, ProjectAction, RecipeMechanicAction,
};
use crate::state::RuntimeError;

/// 校验一条消息里引用的原型名。只校验**本次要写入的字段**——文档里已有的
/// 其它引用（可能来自别的上下文/旧 modpack）不受影响。
pub fn validate_message(store: &PrototypeStore, message: &AppMessage) -> Result<(), RuntimeError> {
    match message {
        AppMessage::Application(_) => Ok(()),
        AppMessage::Project { action, .. } => validate_project(store, action),
        AppMessage::Factory { action, .. } => validate_factory(store, action),
    }
}

fn missing(kind: &str, name: &str) -> RuntimeError {
    RuntimeError::InvalidValue(format!("{kind} {name} 不存在于当前游戏上下文"))
}

fn require(
    store: &PrototypeStore,
    group: PrototypeGroup,
    kind: &str,
    name: &str,
) -> Result<(), RuntimeError> {
    if store.get(group, name).is_some() {
        Ok(())
    } else {
        Err(missing(kind, name))
    }
}

fn require_id(
    store: &PrototypeStore,
    group: PrototypeGroup,
    kind: &str,
    id: &IdWithQuality,
) -> Result<(), RuntimeError> {
    require(store, group, kind, &id.id)?;
    require_quality(store, &id.quality)
}

/// 品质名必须在该仓库的品质顺序里（仓库没有品质原型时不做限制）。
fn require_quality(store: &PrototypeStore, quality: &str) -> Result<(), RuntimeError> {
    let order = store.quality_order();
    if order.is_empty() || order.iter().any(|candidate| candidate == quality) {
        Ok(())
    } else {
        Err(RuntimeError::InvalidValue(format!(
            "品质 {quality} 不存在于当前游戏上下文"
        )))
    }
}

fn require_fuel(store: &PrototypeStore, fuel: &Option<Fuel>) -> Result<(), RuntimeError> {
    match fuel {
        None => Ok(()),
        Some(Fuel::Item { item }) => require_id(store, PrototypeGroup::Item, "物品", item),
        Some(Fuel::Fluid { fluid, .. }) => require(store, PrototypeGroup::Fluid, "流体", fluid),
    }
}

fn validate_project(store: &PrototypeStore, action: &ProjectAction) -> Result<(), RuntimeError> {
    match action {
        ProjectAction::SetQualityLimit { quality } => match quality {
            Some(quality) => require_quality(store, quality),
            None => Ok(()),
        },
        ProjectAction::SetRecipeProductivity { productivity } => {
            require(store, PrototypeGroup::Recipe, "配方", &productivity.recipe)
        }
        ProjectAction::SetInfiniteTechLevel { level } => {
            require(store, PrototypeGroup::Technology, "科技", &level.tech)
        }
        ProjectAction::Planning(planning) => validate_planning(store, planning),
        _ => Ok(()),
    }
}

fn validate_planning(store: &PrototypeStore, action: &PlanningAction) -> Result<(), RuntimeError> {
    match action {
        PlanningAction::AddMachinePreference { machine } => {
            require_id(store, PrototypeGroup::Entity, "机器", machine)
        }
        PlanningAction::AddEnumeratedModule { module } => {
            require_id(store, PrototypeGroup::Item, "插件", module)
        }
        PlanningAction::SetEnumeratedBeacon { plan, .. } => validate_beacon_plan(store, plan),
        PlanningAction::EnumeratedBeaconModule { action, .. } => validate_module(store, action),
        // 「使用最佳插件」的品质由调用方显式给出（运行时不做推断），必须存在于
        // 当前上下文——品质解锁 ≠ 可量产，所以既不默认也不放宽。
        PlanningAction::UseBestModules { quality } => require_quality(store, quality),
        _ => Ok(()),
    }
}

fn validate_beacon_plan(store: &PrototypeStore, plan: &AutoBeaconPlan) -> Result<(), RuntimeError> {
    for beacon in &plan.module_config.beacons {
        require_id(store, PrototypeGroup::Entity, "插件塔", &beacon.beacon)?;
        for (module, _) in &beacon.modules {
            require_id(store, PrototypeGroup::Item, "插件", module)?;
        }
    }
    Ok(())
}

fn validate_factory(store: &PrototypeStore, action: &FactoryAction) -> Result<(), RuntimeError> {
    match action {
        FactoryAction::Context(context) => match context {
            FactoryContextAction::SetPlanet { planet } => match planet {
                Some(planet) => require(store, PrototypeGroup::Planet, "星球", planet),
                None => Ok(()),
            },
            FactoryContextAction::SetSurface { surface } => match surface {
                Some(surface) => require(store, PrototypeGroup::Surface, "地表", surface),
                None => Ok(()),
            },
            FactoryContextAction::SetMajorQuality { quality } => require_quality(store, quality),
            FactoryContextAction::SetDebug { .. } => Ok(()),
        },
        FactoryAction::Mechanic { action, .. } => validate_mechanic(store, action),
        _ => Ok(()),
    }
}

fn validate_mechanic(store: &PrototypeStore, action: &MechanicAction) -> Result<(), RuntimeError> {
    match action {
        MechanicAction::Recipe(action) => match action {
            RecipeMechanicAction::SetRecipe { recipe } => {
                require_id(store, PrototypeGroup::Recipe, "配方", recipe)
            }
            RecipeMechanicAction::SetMachine { machine } => {
                require_id(store, PrototypeGroup::Entity, "机器", machine)
            }
            RecipeMechanicAction::SetFuel { fuel } => require_fuel(store, fuel),
            RecipeMechanicAction::SetFuelTemperature { .. } => Ok(()),
            RecipeMechanicAction::Module(action) => validate_module(store, action),
        },
        MechanicAction::Mining(action) => match action {
            MiningMechanicAction::SetResource { resource } => {
                require(store, PrototypeGroup::Entity, "资源", resource)
            }
            MiningMechanicAction::SetMachine { machine } => {
                require_id(store, PrototypeGroup::Entity, "采矿机", machine)
            }
            MiningMechanicAction::SetFuel { fuel } => require_fuel(store, fuel),
            MiningMechanicAction::SetFuelTemperature { .. } => Ok(()),
            MiningMechanicAction::Module(action) => validate_module(store, action),
        },
        MechanicAction::Spoil(action) => match action {
            crate::message::SpoilMechanicAction::SetItem { item } => {
                require_id(store, PrototypeGroup::Item, "物品", item)
            }
        },
        MechanicAction::Plant(action) => match action {
            crate::message::PlantMechanicAction::SetSeed { seed } => {
                require_id(store, PrototypeGroup::Item, "种子", seed)
            }
        },
        MechanicAction::ItemFuel(action) => match action {
            crate::message::ItemFuelMechanicAction::SetItem { item } => {
                require_id(store, PrototypeGroup::Item, "物品", item)
            }
        },
        MechanicAction::ItemLaunch(action) => match action {
            crate::message::ItemLaunchMechanicAction::SetItem { item } => {
                require_id(store, PrototypeGroup::Item, "物品", item)
            }
            crate::message::ItemLaunchMechanicAction::SetWeightMode { .. } => Ok(()),
        },
        MechanicAction::Generator(action) => match action {
            crate::message::GeneratorMechanicAction::SetGenerator { generator } => {
                require_id(store, PrototypeGroup::Entity, "发电机", generator)
            }
            crate::message::GeneratorMechanicAction::SetFluid { fluid } => {
                require(store, PrototypeGroup::Fluid, "流体", fluid)
            }
            crate::message::GeneratorMechanicAction::SetTemperature { .. } => Ok(()),
        },
        MechanicAction::Boiler(action) => match action {
            crate::message::BoilerMechanicAction::SetBoiler { boiler } => {
                require_id(store, PrototypeGroup::Entity, "锅炉", boiler)
            }
            crate::message::BoilerMechanicAction::SetFluid { fluid } => {
                require(store, PrototypeGroup::Fluid, "流体", fluid)
            }
            crate::message::BoilerMechanicAction::SetTemperature { .. } => Ok(()),
            crate::message::BoilerMechanicAction::SetFuel { fuel } => require_fuel(store, fuel),
            crate::message::BoilerMechanicAction::SetFuelTemperature { .. } => Ok(()),
        },
        MechanicAction::Reactor(action) => match action {
            crate::message::ReactorMechanicAction::SetReactor { reactor } => {
                require_id(store, PrototypeGroup::Entity, "反应堆", reactor)
            }
            crate::message::ReactorMechanicAction::SetFuel { fuel } => require_fuel(store, fuel),
            crate::message::ReactorMechanicAction::SetNeighbours { .. } => Ok(()),
        },
        MechanicAction::Solar(action) => match action {
            crate::message::SolarMechanicAction::SetSolarPanel { solar_panel } => {
                require_id(store, PrototypeGroup::Entity, "太阳能板", solar_panel)
            }
            crate::message::SolarMechanicAction::SetAccumulator { accumulator } => {
                require_id(store, PrototypeGroup::Entity, "蓄电器", accumulator)
            }
        },
        MechanicAction::FluidFuel(action) => match action {
            crate::message::FluidFuelMechanicAction::SetFluid { fluid } => {
                require(store, PrototypeGroup::Fluid, "流体", fluid)
            }
            crate::message::FluidFuelMechanicAction::SetTemperature { .. } => Ok(()),
        },
        MechanicAction::FluidHeat(action) => match action {
            crate::message::FluidHeatMechanicAction::SetFluid { fluid } => {
                require(store, PrototypeGroup::Fluid, "流体", fluid)
            }
            crate::message::FluidHeatMechanicAction::SetTemperature { .. } => Ok(()),
        },
    }
}

fn validate_module(store: &PrototypeStore, action: &ModuleAction) -> Result<(), RuntimeError> {
    match action {
        ModuleAction::SetModuleSlot { module, .. } => match module {
            Some(module) => require_id(store, PrototypeGroup::Item, "插件", module),
            None => Ok(()),
        },
        ModuleAction::AddBeacon { beacon } | ModuleAction::SetBeacon { value: beacon, .. } => {
            require_id(store, PrototypeGroup::Entity, "插件塔", beacon)
        }
        ModuleAction::AddBeaconModule { module, .. }
        | ModuleAction::SetBeaconModule { value: module, .. } => {
            require_id(store, PrototypeGroup::Item, "插件", module)
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store() -> PrototypeStore {
        PrototypeStore::load(&json!({
            "item": {
                "coal": { "type": "item", "name": "coal" },
                "speed-module-3": { "type": "module", "name": "speed-module-3", "category": "speed" }
            },
            "fluid": { "water": { "type": "fluid", "name": "water" } },
            "recipe": {
                "gear": { "type": "recipe", "name": "gear", "energy_required": 1.0,
                          "ingredients": [], "results": [], "enabled": true }
            },
            "assembling-machine": {
                "assembler": { "type": "assembling-machine", "name": "assembler",
                               "crafting_categories": ["crafting"], "crafting_speed": 1.0 }
            },
            "beacon": {
                "beacon": { "type": "beacon", "name": "beacon", "module_slots": 2,
                            "distribution_effectivity": 1.0 }
            },
            "quality": { "normal": { "name": "normal", "level": 0 } },
            "technology": { "tech": { "type": "technology", "name": "tech" } },
            "planet": { "nauvis": { "type": "planet", "name": "nauvis" } }
        }))
        .expect("dump 加载失败")
    }

    fn mechanic_action(action: MechanicAction) -> AppMessage {
        AppMessage::Factory {
            project: crate::id::ProjectId(1),
            factory: crate::id::FactoryId(1),
            action: FactoryAction::Mechanic {
                mechanic: crate::id::MechanicId(1),
                action,
            },
        }
    }

    #[test]
    fn unknown_recipe_and_machine_are_rejected() {
        let store = store();
        let bad_recipe = mechanic_action(MechanicAction::Recipe(RecipeMechanicAction::SetRecipe {
            recipe: IdWithQuality::new("not-a-real-recipe", "normal"),
        }));
        assert!(validate_message(&store, &bad_recipe).is_err());

        let bad_machine =
            mechanic_action(MechanicAction::Recipe(RecipeMechanicAction::SetMachine {
                machine: IdWithQuality::new("not-a-real-machine", "normal"),
            }));
        assert!(validate_message(&store, &bad_machine).is_err());
    }

    #[test]
    fn known_names_and_qualities_pass() {
        let store = store();
        let ok = mechanic_action(MechanicAction::Recipe(RecipeMechanicAction::SetRecipe {
            recipe: IdWithQuality::new("gear", "normal"),
        }));
        assert!(validate_message(&store, &ok).is_ok());

        let bad_quality =
            mechanic_action(MechanicAction::Recipe(RecipeMechanicAction::SetRecipe {
                recipe: IdWithQuality::new("gear", "legendary"),
            }));
        assert!(validate_message(&store, &bad_quality).is_err());

        let fuel = mechanic_action(MechanicAction::Recipe(RecipeMechanicAction::SetFuel {
            fuel: Some(Fuel::Item {
                item: IdWithQuality::new("coal", "normal"),
            }),
        }));
        assert!(validate_message(&store, &fuel).is_ok());

        let bad_fuel = mechanic_action(MechanicAction::Recipe(RecipeMechanicAction::SetFuel {
            fuel: Some(Fuel::Fluid {
                fluid: "not-a-real-fluid".to_string(),
                temperature: None,
            }),
        }));
        assert!(validate_message(&store, &bad_fuel).is_err());
    }

    #[test]
    fn unknown_module_and_beacon_are_rejected() {
        let store = store();
        let bad_module = mechanic_action(MechanicAction::Recipe(RecipeMechanicAction::Module(
            ModuleAction::SetModuleSlot {
                slot: 0,
                module: Some(IdWithQuality::new("not-a-real-module", "normal")),
            },
        )));
        assert!(validate_message(&store, &bad_module).is_err());

        let bad_beacon = mechanic_action(MechanicAction::Recipe(RecipeMechanicAction::Module(
            ModuleAction::AddBeacon {
                beacon: IdWithQuality::new("not-a-real-beacon", "normal"),
            },
        )));
        assert!(validate_message(&store, &bad_beacon).is_err());

        let ok = mechanic_action(MechanicAction::Recipe(RecipeMechanicAction::Module(
            ModuleAction::AddBeacon {
                beacon: IdWithQuality::new("beacon", "normal"),
            },
        )));
        assert!(validate_message(&store, &ok).is_ok());
    }

    #[test]
    fn project_settings_are_validated() {
        let store = store();
        let bad_tech = AppMessage::Project {
            project: crate::id::ProjectId(1),
            action: ProjectAction::SetInfiniteTechLevel {
                level: crate::document::InfiniteTechLevel {
                    tech: "not-a-real-tech".to_string(),
                    level: 1,
                },
            },
        };
        assert!(validate_message(&store, &bad_tech).is_err());

        let bad_recipe = AppMessage::Project {
            project: crate::id::ProjectId(1),
            action: ProjectAction::SetRecipeProductivity {
                productivity: crate::document::RecipeProductivity {
                    recipe: "not-a-real-recipe".to_string(),
                    productivity: 0.1,
                },
            },
        };
        assert!(validate_message(&store, &bad_recipe).is_err());

        let bad_planet = AppMessage::Factory {
            project: crate::id::ProjectId(1),
            factory: crate::id::FactoryId(1),
            action: FactoryAction::Context(FactoryContextAction::SetPlanet {
                planet: Some("not-a-real-planet".to_string()),
            }),
        };
        assert!(validate_message(&store, &bad_planet).is_err());
    }
}
