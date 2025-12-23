use std::{collections::HashMap, env, fmt::Debug, hash::Hash};

use serde_json::Value;

use crate::ctx::{
    ItemLike, factorio::{
        common::{Dict, ItemSubgroup, PrototypeBase}, entity::{ENTITY_TYPES, EntityPrototype}, fluid::FluidPrototype, item::{ITEM_TYPES, ItemPrototype}, mining::{MiningDrillPrototype, ResourcePrototype}, recipe::{CraftingMachinePrototype, RecipePrototype}
    }
};

#[derive(Debug, Clone, Default)]
pub(crate) struct FactorioContext {
    /// 排序参考依据
    pub(crate) groups: Dict<PrototypeBase>,
    pub(crate) subgroups: Dict<ItemSubgroup>,
    /// 被转化的物品集合
    pub(crate) items: Dict<ItemPrototype>,
    pub(crate) entities: Dict<EntityPrototype>,
    pub(crate) fluids: Dict<FluidPrototype>,

    /// 配方类型集合：配方本身和制作配方的机器
    pub(crate) recipes: Dict<RecipePrototype>,
    pub(crate) crafters: Dict<CraftingMachinePrototype>,

    /// 采矿类型集合：资源本身和采矿机器
    pub(crate) resources: Dict<ResourcePrototype>,
    pub(crate) miners: Dict<MiningDrillPrototype>,
}

impl FactorioContext {
    pub(crate) fn load(value: &Value) -> Self {
        let groups: Dict<PrototypeBase> = serde_json::from_value(
            value
                .get("item-group")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        )
        .unwrap();
        let subgroups: Dict<ItemSubgroup> = serde_json::from_value(
            value
                .get("item-subgroup")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        )
        .unwrap();
        let mut items = Dict::<ItemPrototype>::new();
        for item_type in ITEM_TYPES.iter() {
            if let Some(item_values) = value.get(item_type) {
                for item_kv in item_values.as_object().unwrap() {
                    let item: ItemPrototype = serde_json::from_value(item_kv.1.clone()).unwrap();
                    items.insert(item.base.name.clone(), item);
                }
            }
        }
        let mut entities = Dict::<EntityPrototype>::new();
        for entity_type in ENTITY_TYPES.iter() {
            if let Some(entity_values) = value.get(entity_type) {
                for entity_kv in entity_values.as_object().unwrap() {
                    let entity: EntityPrototype =
                        serde_json::from_value(entity_kv.1.clone()).unwrap();
                    entities.insert(entity.base.name.clone(), entity);
                }
            }
        }
        let mut fluids = Dict::<FluidPrototype>::new();
        if let Some(fluid_values) = value.get("fluid") {
            for fluid_kv in fluid_values.as_object().unwrap() {
                let fluid: FluidPrototype = serde_json::from_value(fluid_kv.1.clone()).unwrap();
                fluids.insert(fluid.base.name.clone(), fluid);
            }
        }
        let mut recipes = Dict::<RecipePrototype>::new();
        if let Some(recipe_values) = value.get("recipe") {
            for recipe_kv in recipe_values.as_object().unwrap() {
                let recipe: RecipePrototype = serde_json::from_value(recipe_kv.1.clone()).unwrap();
                recipes.insert(recipe.base.name.clone(), recipe);
            }
        }
        let mut crafters = Dict::<CraftingMachinePrototype>::new();
        for crafter_type in &["assembling-machine", "furnace", "rocket-silo"] {
            if let Some(crafter_values) = value.get(crafter_type) {
                for crafter_kv in crafter_values.as_object().unwrap() {
                    let crafter: CraftingMachinePrototype =
                        serde_json::from_value(crafter_kv.1.clone()).unwrap();
                    crafters.insert(crafter.base.base.name.clone(), crafter);
                }
            }
        }

        let resources: Dict<ResourcePrototype> = serde_json::from_value(
            value
                .get("resource")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        ).unwrap();
        let miners: Dict<MiningDrillPrototype> = serde_json::from_value(
            value
                .get("mining-drill")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        ).unwrap();
        FactorioContext {
            groups,
            subgroups,
            items,
            entities,
            fluids,
            recipes,
            crafters,
            resources,
            miners,
        }
    }

    pub(crate) fn load_from_executable_path(
        executable_path: &std::path::Path,
    ) -> Option<FactorioContext> {
        // 此步较为复杂，调用方应该异步执行
        // 1. 在这个软件的数据文件夹下（秉持绿色原理，创建在这个项目程序本身的同级文件里），创建一个config.cfg
        let self_path = match env::current_dir() {
            Ok(path) => path,
            _ => {
                return None;
            }
        };
        let config_path = self_path.join("tmp/config/config.ini");
        if config_path.exists() == false {
            std::fs::create_dir_all(config_path.parent()?).ok()?;
        }
        // 配置配置文件：写入到自定义的文件夹中避免和运行中的游戏抢锁
        std::fs::write(
            &config_path,
            format!(
                "[path]\nwrite-data={}\n[general]\nlocale=zh-CN",
                self_path.join("tmp").display()
            ),
        )
        .ok()?;
        let dump_raw_command = std::process::Command::new(executable_path)
            .arg("--dump-data")
            .arg("--config")
            .arg(&config_path.to_str().unwrap())
            .output()
            .ok()?;
        if dump_raw_command.status.success() == false {
            return None;
        }
        // let dump_icon_sprites_command = std::process::Command::new(executable_path)
        //     .arg("--dump-icon-sprites")
        //     .arg("--config")
        //     .arg(&config_path.to_str().unwrap())
        //     .output()
        //     .ok()?;
        // if dump_icon_sprites_command.status.success() == false {
        //     return None;
        // }
        let data_raw_dump_json_path = self_path.join("tmp/script-output/data-raw-dump.json");
        let data_str = std::fs::read_to_string(data_raw_dump_json_path).ok()?;
        let value: serde_json::Value = serde_json::from_str(&data_str).ok()?;
        let ctx = FactorioContext::load(&value);
        Some(ctx)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) enum GenericItem {
    Item {
        name: String,
        quality: u8,
    },
    Fluid {
        name: String,
        /// f64 不可 Hash，近似为 i32 表示温度，
        temperature: Option<i32>,
    },
    Entity {
        name: String,
        quality: u8,
    },
    Heat,
    Electricity,
    FluidHeat,
    FluidFuel,
    ItemFuel {
        category: String,
    },
    RocketPayload,
    Custom {
        name: String,
    },
}
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct GenericItemWithLocation {
    base: GenericItem,
    location: u16,
}

impl ItemLike for GenericItem {}
impl ItemLike for GenericItemWithLocation {}

pub(crate) fn make_located_generic_recipe(
    original: HashMap<GenericItem, f64>,
    location: u16,
) -> HashMap<GenericItemWithLocation, f64> {
    let mut located = HashMap::new();
    for (key, value) in original.into_iter() {
        let located_key = GenericItemWithLocation {
            base: key,
            location,
        };
        located.insert(located_key, value);
    }
    located
}

fn sample_five<T: Debug>(map: &Dict<T>) {
    let mut count = 0;
    for (key, value) in map.iter() {
        println!("Key: {}, Value: {:?}", key, value);
        count += 1;
        if count >= 5 {
            break;
        }
    }
}

#[test]
fn test_load_context() {
    let data = include_str!("../../../assets/data-raw-dump.json");
    let value: Value = serde_json::from_str(&data).unwrap();
    let ctx = FactorioContext::load(&value);
    assert!(ctx.items.contains_key("iron-plate"));
    assert!(ctx.entities.contains_key("stone-furnace"));
    assert!(ctx.fluids.contains_key("water"));
    assert!(ctx.recipes.contains_key("iron-gear-wheel"));
    assert!(ctx.crafters.contains_key("assembling-machine-1"));
    // sample 5 for each
    sample_five(&ctx.items);
    sample_five(&ctx.entities);
    sample_five(&ctx.fluids);
    sample_five(&ctx.recipes);
    sample_five(&ctx.crafters);
}

