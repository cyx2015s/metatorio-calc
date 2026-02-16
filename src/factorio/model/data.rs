use std::{
    collections::{HashMap, HashSet},
    env,
    fmt::{Debug, Display},
    hash::Hash,
    io::Write,
    path::PathBuf,
    process::Command,
};

use indexmap::IndexMap;
use serde_json::Value;
use serde_with::{DefaultOnError, serde_as};

use crate::{concept::*, error::AppError, factorio::*};

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

#[derive(Debug, Clone, Default)]
pub struct DataContext {
    /// 模组信息
    pub mods: Vec<(String, String)>,
    /// 图标路径
    pub icon_path: std::path::PathBuf,
    /// 翻译信息
    pub localized_name: Dict<Dict<String>>,
    pub localized_description: Dict<Dict<String>>,

    /// 排序参考依据
    pub groups: Dict<PrototypeBase>,
    pub subgroups: Dict<ItemSubgroup>,

    /// 科技
    pub technologies: Dict<TechnologyPrototype>,

    /// 燃料类型
    pub fuel_categories: Dict<PrototypeBase>,
    pub airborne_pollutants: Dict<PrototypeBase>,

    /// 地点
    pub planets: Dict<PlanetPrototype>,
    pub surface_properties: Dict<SurfacePropertyPrototype>,

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
    pub recipe_categories: Dict<PrototypeBase>,

    /// 采矿类型集合：资源本身和采矿机器
    pub resources: Dict<ResourcePrototype>,
    pub miners: Dict<MiningDrillPrototype>,
    pub resource_categories: Dict<PrototypeBase>,

    /// 流体相关
    pub boilers: Dict<BoilerPrototype>,
    pub generators: Dict<GeneratorPrototype>,
    pub temperatures: Dict<HashSet<i32>>, // 所有流体的*常见*温度列表（出现在filter中指定温度的）

    pub plants: Dict<PlantPrototype>,

    /// 地块
    pub tiles: Dict<TilePrototype>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModInfo {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub enabled: bool,
}

pub fn get_workding_directory() -> PathBuf {
    env::current_exe().unwrap().parent().unwrap().to_path_buf()
}

fn deserialize_type<T>(value: &Value, type_name: &str) -> T
where
    T: serde::de::DeserializeOwned,
{
    let value = value
        .get(type_name)
        .cloned()
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));

    let ret = serde_json::from_value(value.clone());
    match ret {
        Err(err) => {
            eprintln!("解析数据类型 {} 失败: {}", type_name, err);
            eprintln!("原始数据: {}", value);
            panic!("解析数据失败");
        }
        Ok(val) => val,
    }
}

/// 创建 DataContext 的方法
impl DataContext {
    pub fn test_load() -> Self {
        let value = serde_json::from_str::<Value>(
            std::fs::read_to_string("assets/data-raw-dump.json")
                .unwrap()
                .as_str(),
        );
        DataContext::load(&value.unwrap()).build_utility_info()
    }
    pub fn load(value: &Value) -> Self {
        let groups = deserialize_type(value, "item-group");
        let subgroups = deserialize_type(value, "item-subgroup");
        let technologies = deserialize_type(value, "technology");
        let fuel_categories = deserialize_type(value, "fuel-category");
        let airborne_pollutants = deserialize_type(value, "airborne-pollutant");
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
        let fluids = deserialize_type(value, "fluid");
        let recipes = deserialize_type(value, "recipe");
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
        let recipe_categories = deserialize_type(value, "recipe-category");

        let resources = deserialize_type(value, "resource");
        let miners = deserialize_type(value, "mining-drill");
        let resource_categories = deserialize_type(value, "resource-category");
        let modules = deserialize_type(value, "module");

        let beacons = deserialize_type(value, "beacon");
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
        let planets = deserialize_type(value, "planet");
        let tiles = deserialize_type(value, "tile");
        let boilers = deserialize_type(value, "boiler");
        let generators = deserialize_type(value, "generator");
        let plants = deserialize_type(value, "plant");
        log::info!("数据加载完成");
        // ret.planets.iter().for_each(|(_, p)| {
        //     dbg!(p.collect_autoplaced(&ret));
        // });
        DataContext {
            qualities,
            groups,
            subgroups,
            technologies,
            fuel_categories,
            airborne_pollutants,
            items,
            modules,
            beacons,
            entities,
            fluids,
            recipes,
            crafters,
            recipe_categories,
            resources,
            miners,
            resource_categories,
            planets,
            tiles,
            boilers,
            generators,
            plants,
            ..Default::default()
        }
    }

    pub fn load_from_executable_path(
        executable_path: &std::path::Path,
        mod_path: Option<&std::path::Path>,
        lang: Option<&str>,
    ) -> Result<DataContext, AppError> {
        // 此步较为复杂，调用方应该异步执行
        // 1. 在这个软件的数据文件夹下（秉持绿色原理，创建在这个项目程序本身的同级文件里），创建一个config.cfg
        let lang = lang.unwrap_or("zh-CN");
        let self_path = get_workding_directory();
        let config_path = self_path.join("tmp/config/config.ini");
        let tmp_mod_list_json_path = self_path.join("tmp/mods/mod-list.json");
        log::info!("准备创建临时配置文件: {:?}", config_path);
        if tmp_mod_list_json_path.exists() {
            std::fs::remove_file(&tmp_mod_list_json_path)
                .map_err(|err| AppError::ContextCreation(err.to_string()))?;
        }
        if !config_path.exists() {
            std::fs::create_dir_all(config_path.parent().unwrap())
                .map_err(|err| AppError::ContextCreation(err.to_string()))?;
        }
        // 配置配置文件：写入到自定义的文件夹中避免和运行中的游戏抢锁
        let mut config_file = std::fs::File::create(&config_path)?;

        config_file.write_all(b"[path]\nwrite-data=")?;
        config_file.write_all(self_path.join("tmp").as_os_str().as_encoded_bytes())?;
        config_file.write_all(format!("\n[general]\nlocale={}", lang).as_bytes())?;

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
            .output()?;
        if !dump_raw_command.status.success() {
            return Err(AppError::ContextCreation("导出原始数据失败".to_string()));
        }
        log::info!("导出原始数据成功");
        crate::toast::info("导出原始数据成功");
        let dump_locale_command = Command::new(executable_path)
            .arg("--dump-prototype-locale")
            .arg("--config")
            .arg(config_path.to_str().unwrap())
            .args(if let Some(mod_path) = mod_path {
                vec!["--mod-directory", mod_path.to_str().unwrap()]
            } else {
                vec![]
            })
            .output()?;
        if !dump_locale_command.status.success() {
            return Err(AppError::ContextCreation("导出翻译数据失败".to_string()));
        }
        log::info!("导出翻译数据成功");
        crate::toast::info("导出翻译数据成功");

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
            .output()?;
        if !dump_icon_sprites_command.status.success() {
            return Err(AppError::ContextCreation("导出图标数据失败".to_string()));
        }
        log::info!("导出图标数据成功");
        crate::toast::info("导出图标数据成功");

        if let Some(mod_path) = mod_path {
            // 把 mod-list.json 也复制过来
            let mod_list_json_path = mod_path.join("mod-list.json");
            if mod_list_json_path.exists() {
                std::fs::copy(&mod_list_json_path, &tmp_mod_list_json_path)?;
            }
        }
        // 扫描游戏可执行文件下，补充版本信息
        let mut mod_infos_json =
            serde_json::from_str::<Value>(&std::fs::read_to_string(&tmp_mod_list_json_path)?)?;
        let mut mod_infos = serde_json::from_value::<Vec<ModInfo>>(
            mod_infos_json
                .get("mods")
                .ok_or(AppError::ContextCreation(
                    "mod-list.json格式不正确".to_string(),
                ))?
                .clone(),
        )?;
        for mod_info in &mut mod_infos {
            if mod_info.enabled {
                log::info!("处理模组信息 {:?}", mod_info);
                let mod_name = mod_info.name.clone();
                if mod_info.version.is_empty() {
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
                            &std::fs::read_to_string(&info_json_path)?,
                        )?;
                        mod_info.version = info_json_content
                            .get("version")
                            .ok_or(AppError::ContextCreation(
                                "模组的info.json没有version字段".to_string(),
                            ))?
                            .as_str()
                            .ok_or(AppError::ContextCreation(
                                "模组的info.json的version字段不是字符串".to_string(),
                            ))?
                            .to_string();
                        log::info!("模组 {} 的版本是 {}", &mod_name, &mod_info.version);
                    } else {
                        // 在模组路径下寻找info.json
                        log::info!("在模组路径下寻找 {} 的 info.json", mod_name);
                        if mod_path.is_none() {
                            continue;
                        }
                        // 可能是 zip 包
                        for entry in std::fs::read_dir(mod_path.unwrap())? {
                            let entry = entry?;
                            let file_name = entry.file_name().into_string().map_err(|os_err| {
                                AppError::Custom(format!(
                                    "操作系统错误: {}",
                                    os_err.to_string_lossy()
                                ))
                            })?;

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
                                    mod_info.version = version.to_string();
                                    let new_version = version_string_to_triplet(version);
                                    let old_version =
                                        version_string_to_triplet(mod_info.version.as_str());
                                    if old_version < new_version {
                                        mod_info.version = version.to_string();
                                    }
                                    log::info!(
                                        "压缩包模组 {} 的版本是 {}",
                                        &mod_name,
                                        &mod_info.version
                                    );
                                }
                            } else if file_name == mod_name {
                                let info_json_path = entry.path().join("info.json");
                                if !info_json_path.exists() {
                                    // 垃圾文件夹，不用管
                                    continue;
                                }
                                let info_json_content = serde_json::from_str::<Value>(
                                    &std::fs::read_to_string(&info_json_path)?,
                                )?;
                                let version = info_json_content
                                    .get("version")
                                    .ok_or(AppError::ContextCreation(
                                        "模组的info.json没有version字段".to_string(),
                                    ))?
                                    .as_str()
                                    .ok_or(AppError::ContextCreation(
                                        "模组的info.json的version字段不是字符串".to_string(),
                                    ))?;
                                let new_version = version_string_to_triplet(version);
                                let old_version =
                                    version_string_to_triplet(mod_info.version.as_str());
                                if old_version <= new_version {
                                    // 同版本模组，文件优先
                                    mod_info.version = version.to_string();
                                }
                                log::info!("文件模组 {} 的版本是 {}", &mod_name, mod_info.version);

                                break;
                            }
                        }
                    }
                }
            }
        }
        mod_infos_json
            .get_mut("mods")
            .replace(&mut serde_json::to_value(mod_infos)?);
        std::fs::write(
            &tmp_mod_list_json_path,
            serde_json::to_string_pretty(&mod_infos_json)?,
        )?;
        DataContext::load_from_tmp_no_dump()
    }

    pub fn load_from_tmp_no_dump() -> Result<DataContext, AppError> {
        let self_path = get_workding_directory();
        let raw_path = self_path.join("tmp/script-output/data-raw-dump.json");
        let icon_path = self_path.join("tmp/script-output/");
        let json_string = std::fs::read_to_string(&raw_path).map_err(|_| {
            AppError::ContextCreation(format!(
                "读取原始数据文件失败: {:?}",
                raw_path.to_string_lossy()
            ))
        })?;

        let json_value = serde_json::from_str::<Value>(&json_string).map_err(|_| {
            AppError::ContextCreation(format!(
                "解析原始数据文件失败: {:?}",
                raw_path.to_string_lossy()
            ))
        })?;
        let mut factorio = DataContext::load(&json_value);
        factorio.icon_path = icon_path;
        for locale_category in LOCALE_CATEGORIES.iter() {
            log::info!("加载翻译类别 {}", locale_category);
            let locale_path =
                self_path.join(format!("tmp/script-output/{}-locale.json", locale_category));
            if locale_path.exists() {
                // name: a => A, b => B
                // description: a => A desc, b => B desc
                let locale_values: Dict<Dict<String>> = serde_json::from_str(
                    &std::fs::read_to_string(&locale_path).map_err(|_| {
                        AppError::ContextCreation(format!(
                            "读取翻译数据文件失败: {:?}",
                            locale_path
                        ))
                    })?,
                )?;
                factorio.localized_name.insert(
                    locale_category.to_string(),
                    locale_values.get("names").cloned().unwrap_or_default(),
                );
                factorio.localized_description.insert(
                    locale_category.to_string(),
                    locale_values
                        .get("descriptions")
                        .cloned()
                        .unwrap_or_default(),
                );
            } else {
                factorio
                    .localized_name
                    .insert(locale_category.to_string(), Dict::new());
                factorio
                    .localized_description
                    .insert(locale_category.to_string(), Dict::new());
                log::warn!("翻译类别 {} 的文件不存在，跳过", locale_category);
            }
        }
        let mod_list_json_path = self_path.join("tmp/mods/mod-list.json");
        let mod_infos_json =
            serde_json::from_str::<Value>(&std::fs::read_to_string(&mod_list_json_path)?)?;
        let mut mod_infos = serde_json::from_value::<Vec<ModInfo>>(
            mod_infos_json
                .get("mods")
                .ok_or(AppError::ContextCreation(
                    "mod-list.json格式不正确".to_string(),
                ))?
                .clone(),
        )?;
        for mod_info in &mut mod_infos {
            // log::info!("加载模组信息 {:?}", mod_info);
            if mod_info.enabled {
                log::info!("启用模组 {}", &mod_info.name);
                factorio
                    .mods
                    .push((mod_info.name.clone(), mod_info.version.clone()));
            }
        }
        crate::toast::success("加载数据完成");
        Ok(factorio)
    }

    pub fn get_display_name(&self, category: &str, key: &str) -> String {
        self.localized_name
            .get(category)
            .unwrap()
            .get(key)
            .unwrap_or(&format!("{} (unlocalized)", key))
            .to_string()
    }

    pub fn build_utility_info(self) -> Self {
        self.build_order_info().build_temperature_info()
    }

    pub fn build_order_info(mut self) -> Self {
        self.ordered_entries.insert(
            "fuel-category".to_string(),
            get_order_info(&self.fuel_categories, &self.groups, &self.subgroups),
        );
        self.order_of_entries.insert(
            "fuel-category".into(),
            get_reverse_order_info(&self.ordered_entries["fuel-category"]),
        );
        self.ordered_entries.insert(
            "airborne-pollutant".to_string(),
            get_order_info(&self.airborne_pollutants, &self.groups, &self.subgroups),
        );
        self.order_of_entries.insert(
            "airborne-pollutant".into(),
            get_reverse_order_info(&self.ordered_entries["airborne-pollutant"]),
        );
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
            "recipe-category".into(),
            get_order_info(&self.recipe_categories, &self.groups, &self.subgroups),
        );
        self.order_of_entries.insert(
            "recipe-category".into(),
            get_reverse_order_info(&self.ordered_entries["recipe-category"]),
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
        self.ordered_entries.insert(
            "technology".into(),
            get_order_info(&self.technologies, &self.groups, &self.subgroups),
        );
        self.order_of_entries.insert(
            "technology".into(),
            get_reverse_order_info(&self.ordered_entries["technology"]),
        );
        self
    }

    pub fn build_temperature_info(mut self) -> Self {
        for fluid in self.fluids.values() {
            self.temperatures
                .entry(fluid.base.name.clone())
                .or_default()
                .insert(fluid.default_temperature as i32);
        }
        for recipe in self.recipes.values() {
            // 收集配方中所有带定点温度的流体，构建温度信息
            // 忽视输入流体时要求区间范围的配方，这种通常是其他温度协变出来的
            for ingredient in &recipe.ingredients {
                if let RecipeIngredient::Fluid(fluid) = ingredient
                    && let Some(temperature) = fluid.temperature
                {
                    self.temperatures
                        .entry(fluid.name.clone())
                        .or_default()
                        .insert(temperature as i32);
                }
            }
            for result in &recipe.results {
                if let RecipeResult::Fluid(fluid) = result
                    && let Some(temperature) = fluid.temperature
                {
                    self.temperatures
                        .entry(fluid.name.clone())
                        .or_default()
                        .insert(temperature as i32);
                }
            }
        }
        // 机器消耗的带温度筛选的流体，通常应该是其他建筑产生的，不对其做检测了
        // 检测锅炉的目标输出温度即可
        for boiler in self.boilers.values() {
            if let Some(filter) = boiler.output_fluid_box.filter.as_ref()
                && boiler.mode == BoilerMode::OutputToSeparatePipe
            {
                self.temperatures
                    .entry(filter.clone())
                    .or_default()
                    .insert(boiler.target_temperature.unwrap() as i32);
            }
        }
        self
    }
}

fn i32_inf_range() -> [i32; 2] {
    [i32::MIN, i32::MAX]
}

#[serde_as]
#[derive(Debug, Clone, Hash, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum GenericItem {
    Item(IdWithQuality),
    Fluid {
        name: String,
        /// f64 不可 Hash，近似为 i32 表示温度，
        #[serde_as(deserialize_as = "DefaultOnError")]
        #[serde(default = "i32_inf_range")]
        temperature: [i32; 2],
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

impl GenericItem {
    pub fn is_energy(&self) -> bool {
        matches!(
            self,
            GenericItem::Heat
                | GenericItem::Electricity
                | GenericItem::FluidFuel { .. }
                | GenericItem::ItemFuel { .. }
                | GenericItem::FluidHeat { .. }
        )
    }
}

impl Default for GenericItem {
    fn default() -> Self {
        GenericItem::Item("item-unknown".into())
    }
}

impl Display for GenericItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 内部名称毫无意义，这里只显示类型
        write!(
            f,
            "{}",
            match self {
                GenericItem::Item(..) => "物品",
                GenericItem::Fluid { .. } => "流体",
                GenericItem::Entity(..) => "实体",
                GenericItem::Heat => "热能",
                GenericItem::Electricity => "电能",
                GenericItem::FluidHeat { .. } => "流体热源",
                GenericItem::FluidFuel { .. } => "流体燃料",
                GenericItem::ItemFuel { .. } => "物品燃料",
                GenericItem::RocketPayloadWeight => "火箭重量载荷",
                GenericItem::RocketPayloadStack => "火箭堆叠载荷",
                GenericItem::Pollution { .. } => "污染",
                GenericItem::Custom { .. } => "特殊物品",
            }
        )
    }
}

#[derive(Debug, Clone, Default, Hash, PartialEq, Eq)]
pub struct GenericItemWithLocation {
    base: GenericItem,
    location: u16,
}

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
    let factorio = DataContext::test_load().build_utility_info();
    dbg!(&factorio.temperatures);
    assert!(factorio.items.contains_key("iron-plate"));
    assert!(factorio.entities.contains_key("stone-furnace"));
    assert!(factorio.fluids.contains_key("water"));
    assert!(factorio.recipes.contains_key("iron-gear-wheel"));
    assert!(factorio.crafters.contains_key("assembling-machine-1"));
    dbg!(factorio.recipes.get("electronic-circuit"));
    dbg!(factorio.crafters.get("oil-refinery"));

    let water = factorio.fluids.get("water").unwrap();
    let steam = factorio.fluids.get("steam").unwrap();
    let steam_engine = factorio.generators.get("steam-engine").unwrap();
    let steam_turbine = factorio.generators.get("steam-turbine").unwrap();
    let boiler = factorio.boilers.get("boiler").unwrap();
    let heat_exchanger = factorio.boilers.get("heat-exchanger").unwrap();
    dbg!(&boiler);
    dbg!(steam_engine.get_output(steam, 100.0));
    assert!(dbg!(steam_engine.get_output(steam, 165.0)) == (30.0, 900_000.0));
    assert!(dbg!(steam_engine.get_output(steam, 500.0)).0 == 30.0);
    dbg!(steam_turbine.get_output(steam, 165.0));
    dbg!(steam_turbine.get_output(steam, 500.0));
    dbg!(steam_turbine.get_output(steam, 100.0));
    dbg!(boiler.get_flow(&factorio, &water.base.name, 100.0, &None));
    dbg!(boiler.get_flow(&factorio, &water.base.name, 15.0, &None));
    dbg!(boiler.get_flow(
        &factorio,
        &water.base.name,
        15.0,
        &Some(("coal".to_string(), 0))
    ));
    dbg!(heat_exchanger.get_flow(&factorio, &water.base.name, 15.0, &None));
    dbg!(heat_exchanger.get_flow(&factorio, &water.base.name, 50.0, &None));
    dbg!(heat_exchanger.get_flow(&factorio, &water.base.name, 325.0, &None));
    assert!(dbg!(heat_exchanger.get_flow(&factorio, &steam.base.name, 15.0, &None)).is_empty());
}
