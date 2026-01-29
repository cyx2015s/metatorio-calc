use std::{
    collections::HashMap,
    env,
    fmt::Debug,
    hash::Hash,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

use indexmap::IndexMap;
use serde_json::Value;

use crate::{concept::*, factorio::*};

pub const LOCALE_CATEGORIES: &[&str] = &[
    "airborne-pollutant",
    "asteroid-chunk",
    "entity",
    "fluid",
    "fuel-category",
    "item-group",
    "item",
    "quality",
    "recipe",
    "space-location",
    "technology",
    "tile",
];

pub const RAW_JSON: &str = include_str!("../../../assets/data-raw-dump.json");

#[derive(Debug, Clone, Default)]
pub struct FactorioContext {
    /// 模组信息
    pub mods: Vec<(String, String)>,
    /// 图标路径
    pub icon_path: Option<std::path::PathBuf>,
    /// 翻译信息
    pub localized_name: Dict<Dict<String>>,
    pub localized_description: Dict<Dict<String>>,
    /// 排序参考依据
    pub groups: Dict<PrototypeBase>,
    pub subgroups: Dict<ItemSubgroup>,

    /// 地点
    pub planets: Dict<PlanetPrototype>,

    /// 品质
    pub qualities: Vec<QualityPrototype>,

    pub ordered_entries: HashMap<String, OrderInfo>,
    pub order_of_entries: HashMap<String, ReverseOrderInfo>,

    /// 被转化的物品集合
    pub items: Dict<ItemPrototype>,
    pub entities: Dict<EntityPrototype>,
    pub fluids: Dict<FluidPrototype>,

    /// 插件
    pub modules: Dict<ModulePrototype>,
    pub beacons: Dict<BeaconPrototype>,
    /// 配方类型集合：配方本身和制作配方的机器
    pub recipes: Dict<RecipePrototype>,
    pub crafters: Dict<CraftingMachinePrototype>,

    /// 采矿类型集合：资源本身和采矿机器
    pub resources: Dict<ResourcePrototype>,
    pub miners: Dict<MiningDrillPrototype>,

    /// 地块
    pub tiles: Dict<TilePrototype>,
}

pub fn get_workding_directory() -> PathBuf {
    env::current_exe().unwrap().parent().unwrap().to_path_buf()
}

impl FactorioContext {
    pub fn load(value: &Value) -> Self {
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
                items.extend(
                    serde_json::from_value::<Dict<ItemPrototype>>(item_values.clone()).unwrap(),
                );
            }
        }
        let mut entities = Dict::<EntityPrototype>::new();
        for entity_type in ENTITY_TYPES.iter() {
            if let Some(entity_values) = value.get(entity_type) {
                entities.extend(
                    serde_json::from_value::<Dict<EntityPrototype>>(entity_values.clone()).unwrap(),
                );
            }
        }
        let fluids: Dict<FluidPrototype> = serde_json::from_value(
            value
                .get("fluid")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        )
        .unwrap();
        let recipes: Dict<RecipePrototype> = serde_json::from_value(
            value
                .get("recipe")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        )
        .unwrap();
        let mut crafters = Dict::<CraftingMachinePrototype>::new();
        for crafter_type in CRAFTING_MACHINE_TYPES.iter() {
            if let Some(crafter_values) = value.get(crafter_type) {
                crafters.extend(
                    serde_json::from_value::<Dict<CraftingMachinePrototype>>(
                        crafter_values.clone(),
                    )
                    .unwrap(),
                );
            }
        }

        let resources: Dict<ResourcePrototype> = serde_json::from_value(
            value
                .get("resource")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        )
        .unwrap();
        let miners: Dict<MiningDrillPrototype> = serde_json::from_value(
            value
                .get("mining-drill")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        )
        .unwrap();
        let modules: Dict<ModulePrototype> = serde_json::from_value(
            value
                .get("module")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        )
        .unwrap();
        let beacons: Dict<BeaconPrototype> = serde_json::from_value(
            value
                .get("beacon")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        )
        .unwrap();
        let mut qualities = vec![];
        let mut cur_quality = value.get("quality").unwrap().get("normal").unwrap();
        while !cur_quality.is_null() {
            let quality: QualityPrototype = serde_json::from_value(cur_quality.clone()).unwrap();
            qualities.push(quality.clone());
            cur_quality = value
                .get("quality")
                .unwrap()
                .get(quality.next.as_ref().unwrap_or(&"".to_string()))
                .unwrap_or(&Value::Null)
        }
        for entity in entities.values() {
            if let Some(autoplace) = &entity.autoplace
                && (entity.base.r#type == "resource")
            {
                log::info!("自动生成的资源: {}", &entity.base.name);
                if !autoplace.control.is_empty() {
                    log::info!(" ↑ 对应的控制 ID 为 {}", &autoplace.control);
                }
            }
        }
        let planets: Dict<PlanetPrototype> = serde_json::from_value(
            value
                .get("planet")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        )
        .unwrap();
        let tiles: Dict<TilePrototype> = serde_json::from_value(
            value
                .get("tile")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        )
        .unwrap();
        let ret = FactorioContext {
            qualities,
            groups,
            subgroups,
            items,
            modules,
            beacons,
            entities,
            fluids,
            recipes,
            crafters,
            resources,
            miners,
            icon_path: None,
            planets,
            tiles,
            ..Default::default()
        };
        ret.planets.iter().for_each(|(_, p)| {
            dbg!(p.collect_autoplaced(&ret));
        });
        ret
    }

    pub fn load_from_executable_path(
        executable_path: &std::path::Path,
        mod_path: Option<&std::path::Path>,
        lang: Option<&str>,
    ) -> Option<FactorioContext> {
        // 此步较为复杂，调用方应该异步执行
        // 1. 在这个软件的数据文件夹下（秉持绿色原理，创建在这个项目程序本身的同级文件里），创建一个config.cfg
        let lang = lang.unwrap_or("zh-CN");
        let self_path = get_workding_directory();
        let config_path = self_path.join("tmp/config/config.ini");
        let tmp_mod_list_json_path = self_path.join("tmp/mods/mod-list.json");
        log::info!("准备创建临时配置文件: {:?}", config_path);
        if tmp_mod_list_json_path.exists() {
            std::fs::remove_file(&tmp_mod_list_json_path).ok()?;
        }
        if !config_path.exists() {
            std::fs::create_dir_all(config_path.parent()?).ok()?;
        }
        // 配置配置文件：写入到自定义的文件夹中避免和运行中的游戏抢锁
        let mut config_file = std::fs::File::create(&config_path).ok()?;

        config_file.write_all(b"[path]\nwrite-data=").ok()?;
        config_file
            .write_all(self_path.join("tmp").as_os_str().as_encoded_bytes())
            .ok()?;
        config_file
            .write_all(format!("\n[general]\nlocale={}", lang).as_bytes())
            .ok()?;

        log::info!("创建 config.ini 成功");
        let dump_raw_command = Command::new(executable_path)
            .arg("--dump-data")
            .arg("--config")
            .arg(config_path.to_str().unwrap())
            .args(if let Some(mod_path) = mod_path {
                vec!["--mod-directory", mod_path.to_str().unwrap()]
            } else {
                vec![]
            })
            .stdout(Stdio::null())
            .output()
            .ok()?;
        if !dump_raw_command.status.success() {
            return None;
        }
        log::info!("导出原始数据成功");
        let dump_locale_command = Command::new(executable_path)
            .arg("--dump-prototype-locale")
            .arg("--config")
            .arg(config_path.to_str().unwrap())
            .args(if let Some(mod_path) = mod_path {
                vec!["--mod-directory", mod_path.to_str().unwrap()]
            } else {
                vec![]
            })
            .stdout(Stdio::null())
            .output()
            .ok()?;
        if !dump_locale_command.status.success() {
            return None;
        }
        log::info!("导出翻译数据成功");

        let dump_icon_sprites_command = Command::new(executable_path)
            .arg("--dump-icon-sprites")
            .arg("--disable-audio")
            .arg("--config")
            .arg(config_path.to_str().unwrap())
            .args(if let Some(mod_path) = mod_path {
                vec!["--mod-directory", mod_path.to_str().unwrap()]
            } else {
                vec![]
            })
            .stdout(Stdio::null())
            .output()
            .ok()?;
        if !dump_icon_sprites_command.status.success() {
            return None;
        }
        log::info!("导出图标数据成功");

        if let Some(mod_path) = mod_path {
            // 把 mod-list.json 也复制过来
            let mod_list_json_path = mod_path.join("mod-list.json");
            if mod_list_json_path.exists() {
                std::fs::copy(&mod_list_json_path, &tmp_mod_list_json_path).ok()?;
            }
        }
        // 扫描游戏可执行文件下，补充版本信息
        let mut mod_list_json_content =
            serde_json::from_str::<Value>(&std::fs::read_to_string(&tmp_mod_list_json_path).ok()?)
                .ok()?;
        for mod_info in mod_list_json_content.get_mut("mods")?.as_array_mut()? {
            if mod_info.get("enabled")?.as_bool()? {
                log::info!("处理模组信息 {:?}", mod_info);
                let mod_name = mod_info.get("name")?.as_str()?.to_string();
                if mod_info.get("version").is_none() {
                    log::info!("模组 {} 缺少版本信息，尝试补全", &mod_name);

                    if ["base", "space-age", "quality", "elevated-rails"]
                        .contains(&mod_name.as_str())
                    {
                        // 在游戏可执行文件附近寻找info.json
                        log::info!("在游戏可执行文件附近寻找info.json");
                        let info_json_path = executable_path
                            .join("../../../data")
                            .join(&mod_name)
                            .join("info.json");
                        let info_json_content = serde_json::from_str::<Value>(
                            &std::fs::read_to_string(&info_json_path).ok()?,
                        );
                        mod_info["version"] = info_json_content.ok()?.get("version")?.clone();
                        log::info!("模组 {} 的版本是 {}", &mod_name, &mod_info["version"]);
                    } else {
                        // 在模组路径下寻找info.json
                        log::info!("在模组路径下寻找 {} 的 info.json", mod_name);
                        if mod_path.is_none() {
                            continue;
                        }
                        // 可能是 zip 包
                        for entry in std::fs::read_dir(mod_path.unwrap()).ok()? {
                            let entry = entry.ok()?;
                            let file_name = entry.file_name().into_string().ok()?;

                            if file_name.starts_with(format!("{}_", &mod_name).as_str())
                                && file_name.ends_with(".zip")
                            {
                                log::info!("可能匹配的文件：{}", file_name);
                                log::info!(
                                    "模组 {} 是压缩包，尝试从压缩包文件名读取版本",
                                    &mod_name
                                );
                                let version_str = file_name.split("_").last();
                                if let Some(version_str) = version_str {
                                    let version = version_str.trim_end_matches(".zip");
                                    mod_info["version"] = Value::String(version.to_string());
                                    let new_version = version_string_to_triplet(version);
                                    let old_version = version_string_to_triplet(
                                        mod_info["version"].as_str().unwrap_or("0.0.0"),
                                    );
                                    if old_version < new_version {
                                        mod_info["version"] = Value::String(version.to_string());
                                    }
                                    log::info!(
                                        "压缩包模组 {} 的版本是 {}",
                                        &mod_name,
                                        &mod_info["version"]
                                    );
                                }
                            } else if file_name == mod_name {
                                let info_json_path = entry.path().join("info.json");
                                if !info_json_path.exists() {
                                    // 垃圾文件夹，不用管
                                    continue;
                                }
                                let info_json_content = serde_json::from_str::<Value>(
                                    &std::fs::read_to_string(&info_json_path).ok()?,
                                );
                                let version = info_json_content
                                    .unwrap()
                                    .get("version")?
                                    .as_str()?
                                    .to_owned();
                                let new_version = version_string_to_triplet(&version);
                                let old_version = version_string_to_triplet(
                                    mod_info["version"].as_str().unwrap_or("0.0.0"),
                                );
                                if old_version <= new_version {
                                    // 同版本模组，文件优先
                                    mod_info["version"] = Value::String(version.to_string());
                                }
                                log::info!(
                                    "文件模组 {} 的版本是 {}",
                                    &mod_name,
                                    mod_info["version"]
                                );

                                break;
                            }
                        }
                    }
                }
            }
        }
        std::fs::write(
            &tmp_mod_list_json_path,
            serde_json::to_string_pretty(&mod_list_json_content).ok()?,
        )
        .ok()?;
        FactorioContext::load_from_tmp_no_dump()
    }

    pub fn load_from_tmp_no_dump() -> Option<FactorioContext> {
        let self_path = get_workding_directory();
        let raw_path = self_path.join("tmp/script-output/data-raw-dump.json");
        let icon_path = self_path.join("tmp/script-output/");
        let json_string = std::fs::read_to_string(&raw_path);
        if json_string.is_err() {
            log::error!("读取原始数据文件失败: {:?}", raw_path);
            return None;
        }
        let json_value = serde_json::from_str::<Value>(&json_string.unwrap());
        if json_value.is_err() {
            log::error!("解析原始数据文件失败: {:?}", raw_path);
            return None;
        }
        let mut ctx = FactorioContext::load(&json_value.unwrap());
        ctx.icon_path = Some(icon_path);
        for locale_category in LOCALE_CATEGORIES.iter() {
            log::info!("加载翻译类别 {}", locale_category);
            let locale_path =
                self_path.join(format!("tmp/script-output/{}-locale.json", locale_category));
            if locale_path.exists() {
                // name: a => A, b => B
                // description: a => A desc, b => B desc
                let locale_values: Dict<Dict<String>> =
                    serde_json::from_str(&std::fs::read_to_string(&locale_path).ok()?).ok()?;
                ctx.localized_name.insert(
                    locale_category.to_string(),
                    locale_values.get("names").cloned().unwrap_or_default(),
                );
                ctx.localized_description.insert(
                    locale_category.to_string(),
                    locale_values
                        .get("descriptions")
                        .cloned()
                        .unwrap_or_default(),
                );
            } else {
                ctx.localized_name
                    .insert(locale_category.to_string(), Dict::new());
                ctx.localized_description
                    .insert(locale_category.to_string(), Dict::new());
                log::warn!("翻译类别 {} 的文件不存在，跳过", locale_category);
            }
        }
        let mod_list_json_path = self_path.join("tmp/mods/mod-list.json");
        let mut mod_list_json_content =
            serde_json::from_str::<Value>(&std::fs::read_to_string(&mod_list_json_path).ok()?)
                .ok()?;
        for mod_info in mod_list_json_content.get_mut("mods")?.as_array_mut()? {
            // log::info!("加载模组信息 {:?}", mod_info);
            if mod_info.get("enabled")?.as_bool()? {
                let mod_name = mod_info.get("name")?.as_str()?.to_string();
                log::info!("启用模组 {}", &mod_name);
                ctx.mods.push((
                    mod_name,
                    mod_info
                        .get("version")
                        .map_or("unknown", |v| v.as_str().unwrap_or("unknown"))
                        .to_string(),
                ));
            }
        }
        Some(ctx)
    }

    pub fn get_display_name(&self, category: &str, key: &str) -> String {
        self.localized_name
            .get(category)
            .unwrap()
            .get(key)
            .unwrap_or(&format!("{} (unlocalized)", key))
            .to_string()
    }

    pub fn build_order_info(mut self) -> Self {
        self.ordered_entries.insert(
            "item".to_string(),
            get_order_info(&self.items, &self.groups, &self.subgroups),
        );
        self.order_of_entries.insert(
            "item".into(),
            get_reverse_order_info(&self.ordered_entries["item"]),
        );
        // 没有 order 的 recipe 的 order 从 item 派生
        // md 长见识了，怎么还有不设置 group 和 subgroup 的配方
        for (recipe_name, recipe) in self.recipes.iter_mut() {
            if (recipe.base.order.is_empty() || recipe.base.subgroup.is_empty())
                && !recipe.base.hidden
            {
                if recipe.results.len() == 1 {
                    match recipe.results[0] {
                        RecipeResult::Item(ref r) => {
                            if let Some(item) = self.items.get(&r.name) {
                                recipe.base.subgroup = item.base.subgroup.clone();
                                recipe.base.order = item.base.order.clone();
                            }
                        }
                        RecipeResult::Fluid(ref f) => {
                            if let Some(fluid) = self.fluids.get(&f.name) {
                                recipe.base.subgroup = fluid.base.subgroup.clone();
                                recipe.base.order = fluid.base.order.clone();
                            }
                        }
                    }
                } else if let Some(main_product) = &recipe.main_product {
                    if let Some(item) = self.items.get(main_product) {
                        recipe.base.subgroup = item.base.subgroup.clone();
                        recipe.base.order = item.base.order.clone();
                    }
                } else {
                    // 如果有和配方名相同的物品，则使用该物品的信息
                    for result in &recipe.results {
                        match result {
                            RecipeResult::Item(r) => {
                                if r.name == *recipe_name
                                    && let Some(item) = self.items.get(&r.name)
                                {
                                    recipe.base.subgroup = item.base.subgroup.clone();
                                    recipe.base.order = item.base.order.clone();
                                }
                            }
                            RecipeResult::Fluid(f) => {
                                if f.name == *recipe_name
                                    && let Some(fluid) = self.fluids.get(&f.name)
                                {
                                    recipe.base.subgroup = fluid.base.subgroup.clone();
                                    recipe.base.order = fluid.base.order.clone();
                                }
                            }
                        }
                    }
                }
            }
        }
        self.ordered_entries.insert(
            "recipe".into(),
            get_order_info(&self.recipes, &self.groups, &self.subgroups),
        );
        self.order_of_entries.insert(
            "recipe".into(),
            get_reverse_order_info(&self.ordered_entries["recipe"]),
        );
        self.ordered_entries.insert(
            "fluid".into(),
            get_order_info(&self.fluids, &self.groups, &self.subgroups),
        );
        self.order_of_entries.insert(
            "fluid".into(),
            get_reverse_order_info(&self.ordered_entries["fluid"]),
        );
        // 没有 order 的 entity，从 item 派生
        for (entity_name, entity) in self.entities.iter_mut() {
            for item in self.items.values() {
                if item.place_result.as_ref() == Some(entity_name) {
                    entity.base.subgroup = item.base.subgroup.clone();
                    entity.base.order = item.base.order.clone();
                }
            }
        }
        self.ordered_entries.insert(
            "entity".into(),
            get_order_info(&self.entities, &self.groups, &self.subgroups),
        );
        self.order_of_entries.insert(
            "entity".into(),
            get_reverse_order_info(&self.ordered_entries["entity"]),
        );
        self
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GenericItem {
    Item(IdWithQuality),
    Fluid {
        name: String,
        /// f64 不可 Hash，近似为 i32 表示温度，
        temperature: Option<i32>,
    },
    Entity(IdWithQuality),
    Heat,
    Electricity,
    /// 带筛选功能的流体热源
    /// None 表示任意流体，可以从任意带筛选的流体热源中获取
    FluidHeat {
        filter: Option<String>,
    },
    /// 带筛选功能的流体燃料
    /// None 表示任意流体，可以从任意带筛选的流体燃料中获取
    FluidFuel {
        filter: Option<String>,
    },
    ItemFuel {
        category: String,
    },
    RocketPayloadWeight,
    RocketPayloadStack,
    Pollution {
        name: String,
    },
    Custom {
        name: String,
    },
}
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GenericItemWithLocation {
    base: GenericItem,
    location: u16,
}

impl ItemIdent for GenericItem {}
impl ItemIdent for GenericItemWithLocation {}

pub fn make_located_generic_recipe(
    original: Flow<GenericItem>,
    location: u16,
) -> Flow<GenericItemWithLocation> {
    let mut located = IndexMap::new();
    for (key, value) in original.into_iter() {
        let located_key = GenericItemWithLocation {
            base: key,
            location,
        };
        located.insert(located_key, value);
    }
    located
}

#[test]
fn test_load_context() {
    let ctx = FactorioContext::default();
    assert!(ctx.items.contains_key("iron-plate"));
    assert!(ctx.entities.contains_key("stone-furnace"));
    assert!(ctx.fluids.contains_key("water"));
    assert!(ctx.recipes.contains_key("iron-gear-wheel"));
    assert!(ctx.crafters.contains_key("assembling-machine-1"));
    dbg!(ctx.recipes.get("electronic-circuit"));
    dbg!(ctx.crafters.get("oil-refinery"));
}
