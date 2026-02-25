use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
};

use serde_with::{DefaultOnError, serde_as};

use crate::{
    comb::Compositions,
    concept::*,
    factorio::{
        DataContext, ModulePrototype, ProjectContext, SurfaceCondition,
        common::*,
        editor::icon::Icon,
        modal::SelectorModal,
        model::{
            data::GenericItem,
            energy::energy_source_as_flow,
            entity::EntityPrototype,
            module::{ModuleConfig, ModuleConfigEditor},
            quality::calc_quality_distribution,
        },
        module_effects_allowed,
        planner::FactoryContext,
        selector::Selector,
        surface_condition_satisfied,
    },
    math::ElemVec,
};

fn always_true() -> bool {
    true
}

fn always_half() -> f64 {
    0.5
}

fn always_one() -> f64 {
    1.0
}

fn always_three() -> f64 {
    3.0
}

#[serde_as]
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct RecipePrototype {
    #[serde(flatten)]
    pub base: PrototypeBase,

    category: Option<String>,
    #[serde_as(deserialize_as = "DefaultOnError")]
    surface_conditions: Vec<SurfaceCondition>,

    #[serde_as(deserialize_as = "DefaultOnError")]
    additional_categories: Vec<String>,

    #[serde_as(deserialize_as = "DefaultOnError")]
    #[serde(default)]
    pub ingredients: Vec<RecipeIngredient>,

    #[serde_as(deserialize_as = "DefaultOnError")]
    #[serde(default)]
    pub results: Vec<RecipeResult>,
    pub main_product: Option<String>,

    #[serde_as(deserialize_as = "DefaultOnError")]
    #[serde(default)]
    pub allowed_module_categories: Option<Vec<String>>,

    /// 制作时间（秒）
    #[serde(default = "always_half")]
    pub energy_required: f64,

    /// 配方污染倍数
    #[serde(default = "always_one")]
    pub emissions_multiplier: f64,

    /// 最大产能加成
    #[serde(default = "always_three")]
    pub maximum_productivity: f64,

    /// 开局是否可用
    #[serde(default = "always_true")]
    pub enabled: bool,

    /// 产物若为可变质，是否永远新鲜
    pub result_is_always_fresh: bool,

    /// 是否允许使用降低能耗的插件
    #[serde(default = "always_true")]
    pub allow_consumption: bool,

    /// 是否允许使用增加速度的插件
    #[serde(default = "always_true")]
    pub allow_speed: bool,

    /// 是否允许使用增加产能的插件
    pub allow_productivity: bool,

    /// 是否允许使用降低污染的插件
    #[serde(default = "always_true")]
    pub allow_pollution: bool,

    /// 是否允许使用增加品质的插件
    #[serde(default = "always_true")]
    pub allow_quality: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RecipeIngredient {
    /// 物品原料
    Item(ItemIngredient),
    /// 流体原料
    Fluid(FluidIngredient),
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ItemIngredient {
    pub name: String,
    pub amount: f64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FluidIngredient {
    pub name: String,
    pub amount: f64,
    pub temperature: Option<f64>,
    pub minimum_temperature: Option<f64>,
    pub maximum_temperature: Option<f64>,
    pub fluidbox_index: Option<f64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RecipeResult {
    /// 物品产物
    Item(ItemResult),
    /// 流体产物
    Fluid(FluidResult),
}

impl HasPrototypeBase for RecipePrototype {
    fn base(&self) -> &PrototypeBase {
        &self.base
    }
}

#[derive(Clone, serde::Deserialize)]
#[serde(default)]
pub struct ItemResult {
    pub name: String,
    pub amount: Option<f64>,
    pub amount_min: Option<f64>,
    pub amount_max: Option<f64>,
    pub probability: f64,
    pub ignored_by_stats: Option<f64>,
    pub ignored_by_productivity: Option<f64>,
    pub extra_count_fraction: f64,
    pub percent_spoiled: f64,
}

impl Default for ItemResult {
    fn default() -> Self {
        ItemResult {
            name: String::new(),
            amount: None,
            amount_min: None,
            amount_max: None,
            probability: 1.0,
            ignored_by_stats: None,
            ignored_by_productivity: None,
            extra_count_fraction: 0.0,
            percent_spoiled: 0.0,
        }
    }
}

impl Debug for ItemResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (base_yield, extra_yield) = self.normalized_output();
        f.debug_struct("ItemResult")
            .field("name", &self.name)
            .field("<base yield>", &base_yield)
            .field("<productivity yield>", &extra_yield)
            .field("percent_spoiled", &self.percent_spoiled)
            .finish()
    }
}

impl ItemResult {
    /// 计算当前配方的实际单次产量和每次结算产能加成时的额外产量
    pub fn normalized_output(&self) -> (f64, f64) {
        let extra = self.extra_count_fraction;
        let prob = self.probability;
        let ignore = match self.ignored_by_productivity {
            Some(value) => value,
            None => self.ignored_by_stats.unwrap_or(0.0),
        }
        .floor();
        match self.amount {
            Some(amount) => {
                // 产出分别为：
                // amount (prob * (1 - extra))
                // amount + 1 (prob * extra)
                // 1 (1 - prob * extra)
                let base = amount.floor();
                let productivity = f64::max((base - ignore) * prob * (1.0 - extra), 0.0)
                    + f64::max((base + 1.0 - ignore) * prob * extra, 0.0)
                    + f64::max((1.0 - ignore) * (1.0 - prob) * extra, 0.0);
                (base * prob + extra, productivity)
            }
            None => {
                // 产出分别为：
                // min ~ max (prob * (1 - extra))
                // (min ~ max) + 1 (prob * extra)
                // 1 (1 - prob * extra)
                // 减去 ignore 前要先判断范围，还要求平均
                let min = self.amount_min.unwrap_or(0.0).floor();
                let max = match self.amount_max {
                    Some(value) => value,
                    None => min,
                }
                .floor();
                let max = f64::max(max, min);

                let productivity = f64::max(
                    // 首项加末项乘项数除以状态数乘概率除以二
                    (max - ignore + f64::max(min - ignore, 0.0))
                        * (max - f64::max(min - ignore, 0.0) + 1.0)
                        / (max - min + 1.0)
                        / 2.0
                        * prob
                        * (1.0 - extra),
                    0.0,
                ) + f64::max(
                    (max + 1.0 - ignore + f64::max(min + 1.0 - ignore, 0.0))
                        * (max - f64::max(min + 1.0 - ignore, 0.0) + 1.0)
                        / (max - min + 1.0)
                        / 2.0
                        * prob
                        * extra,
                    0.0,
                ) + f64::max((extra - ignore) * (1.0 - prob) * extra, 0.0);
                (((max + min) / 2.0) * prob + extra, productivity)
            }
        }
    }
}

#[derive(Clone, serde::Deserialize)]
#[serde(default)]
pub struct FluidResult {
    pub name: String,
    pub amount: Option<f64>,
    pub amount_min: Option<f64>,
    pub amount_max: Option<f64>,
    pub probability: f64,
    pub ignored_by_stats: Option<f64>,
    pub ignored_by_productivity: Option<f64>,
    pub temperature: Option<f64>,
    pub min_temperature: Option<f64>,
    pub max_temperature: Option<f64>,
    pub fluidbox_index: f64,
}

impl Default for FluidResult {
    fn default() -> Self {
        FluidResult {
            name: String::new(),
            amount: None,
            amount_min: None,
            amount_max: None,
            probability: 1.0,
            ignored_by_stats: None,
            ignored_by_productivity: None,
            temperature: None,
            min_temperature: None,
            max_temperature: None,
            fluidbox_index: 0.0,
        }
    }
}

impl Debug for FluidResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (base_yield, extra_yield) = self.normalized_output();
        f.debug_struct("FluidResult")
            .field("name", &self.name)
            .field("<base yield>", &base_yield)
            .field("<productivity yield>", &extra_yield)
            .field("temperature", &self.temperature)
            .finish()
    }
}

impl FluidResult {
    /// 计算当前配方的实际单词产量和每次结算产能加成时的额外产量
    pub fn normalized_output(&self) -> (f64, f64) {
        let prob = self.probability;
        let ignore = match self.ignored_by_productivity {
            Some(value) => value,
            None => self.ignored_by_stats.unwrap_or(0.0),
        };
        match self.amount {
            Some(amount) => {
                let base = amount;
                let productivity = f64::max((base - ignore) * prob, 0.0);
                (base * prob, productivity)
            }
            None => {
                let min = self.amount_min.unwrap_or(0.0);
                let max = match self.amount_max {
                    Some(value) => value,
                    None => min,
                };
                let max = f64::max(max, min);
                let productivity = f64::max(
                    // 积分均值
                    (max - ignore + f64::max(min - ignore, 0.0))
                        * (max - f64::max(min - ignore, 0.0))
                        / 2.0
                        / (max - min)
                        * prob,
                    0.0,
                );
                (((max + min) / 2.0) * prob, productivity)
            }
        }
    }
}

pub const CRAFTING_MACHINE_TYPES: &[&str] = &["assembling-machine", "furnace", "rocket-silo"];

#[serde_as]
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CraftingMachinePrototype {
    #[serde(flatten)]
    pub base: EntityPrototype,
    #[serde(default)]
    pub quality_affects_energy_usage: bool,
    #[serde(default)]
    pub energy_usage: Option<EnergyAmount>,
    #[serde(default)]
    pub crafting_speed: f64,

    #[serde_as(deserialize_as = "DefaultOnError")]
    pub crafting_categories: Vec<String>,

    pub energy_source: EnergySource,
    #[serde(default)]
    pub effect_receiver: Option<EffectReceiver>,
    #[serde(default)]
    pub module_slots: f64,
    #[serde(default)]
    pub quality_affects_module_slots: bool,

    #[serde(default)]
    pub allowed_effects: Option<EffectTypeLimitation>,
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub allowed_module_categories: Option<Vec<String>>,
    #[serde(default)]
    pub crafting_speed_quality_multiplier: Option<Dict<f64>>,
    #[serde(default)]
    pub module_slots_quality_bonus: Option<Dict<f64>>,
    #[serde(default)]
    pub energy_usage_quality_multiplier: Option<Dict<f64>>,

    pub fixed_recipe: Option<String>,
    pub fixed_quality: Option<String>,
    #[serde(alias = "source_inventory_size", alias = "ingredient_count")]
    pub input_limit: Option<f64>,
    #[serde(alias = "result_inventory_size", alias = "max_item_product_count")]
    pub output_limit: Option<f64>,

    #[serde(default)]
    pub launch_to_space_platforms: bool,
    #[serde(default)]
    pub to_be_inserted_to_rocket_inventory_size: f64,
    #[serde(default)]
    pub rocket_parts_required: f64,
}

impl HasPrototypeBase for CraftingMachinePrototype {
    fn base(&self) -> &PrototypeBase {
        &self.base.base
    }
}

pub fn machine_fits_for_recipe(
    crafter: &CraftingMachinePrototype,
    recipe: &RecipePrototype,
) -> bool {
    if crafter
        .crafting_categories
        .contains(recipe.category.as_ref().unwrap_or(&"crafting".to_string()))
    {
        return true;
    }
    if recipe
        .additional_categories
        .iter()
        .any(|cat| crafter.crafting_categories.contains(cat))
    {
        return true;
    }
    false
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct RecipeInstance {
    pub recipe: IdWithQuality,
    pub machine: IdWithQuality,
    pub module_config: ModuleConfig,

    /// 当机器的能源类型为Fluid、Burner时，用统一的抽象能源还是用具体的燃料
    /// 类型为Electric、Heat、Void时无效
    /// 类型为Fluid时，值为(流体名, 流体温度)
    /// 类型为Burner时，值为(物品名, 物品品质)
    pub fuel: Option<(String, i32)>,
}

impl SolveContext for RecipeInstance {
    type Game = DataContext;
    type Item = GenericItem;
}

impl Default for RecipeInstance {
    fn default() -> Self {
        RecipeInstance {
            recipe: ("recipe-unknown".to_string(), 0).into(),
            machine: ("entity-unknown".to_string(), 0).into(),
            module_config: ModuleConfig::new(),
            fuel: None,
        }
    }
}

impl AsFlow for RecipeInstance {
    fn as_flow(
        &self,
        data: &DataContext,
        proj: &ProjectContext,
        _factory: &FactoryContext,
    ) -> Flow<Self::Item> {
        let mut map = Flow::new();

        let mut module_effects = self.module_config.get_effect(data);

        if let Some(productivity_bonus) = proj.recipe_productivity.get(&self.recipe.0) {
            module_effects.productivity += *productivity_bonus;
        }

        let mut base_speed = 1.0;

        let mut is_rocket = false;
        let mut by_weight = false;
        let mut stacks = 0;
        let mut rocket_parts_required = 0.0;
        let crafter = data.crafters.get(&self.machine.0);

        if let Some(crafter) = crafter {
            if &crafter.base.base.r#type == "rocket-silo" {
                is_rocket = true;
                stacks = crafter.to_be_inserted_to_rocket_inventory_size as u16;
                rocket_parts_required = crafter.rocket_parts_required as u32 as f64;
                if crafter.launch_to_space_platforms {
                    by_weight = true;
                }
            }
            module_effects = module_effects
                + crafter
                    .effect_receiver
                    .clone()
                    .unwrap_or_default()
                    .base_effect
                    .clone();
            module_effects = module_effects.clamped();
            base_speed = crafter.crafting_speed;
            let quality_level = self.machine.1 as usize;
            if let Some(multiplier) = &crafter.crafting_speed_quality_multiplier {
                let quality = &data.qualities[quality_level].base.name;
                let speed_multiplier = multiplier.get(quality).cloned().unwrap_or(1.0);
                base_speed *= speed_multiplier;
            } else {
                let quality = &data.qualities[quality_level];
                base_speed *= quality.crafting_machine_speed_multiplier();
            }
            let energy_usage = crafter
                .energy_usage
                .as_ref()
                .expect("CraftingMachinePrototype 中的机器没有能量消耗");

            let energy_related_flow = energy_source_as_flow(
                data,
                &crafter.energy_source,
                energy_usage,
                &module_effects,
                &self.fuel,
                &mut base_speed,
            );
            if let EnergySource::Electric(e) = &crafter.energy_source
                && e.drain.is_none()
            {
                // 没有写drain的组装机，按照常态能量消耗的1/30计算drain
                index_map_update_entry(
                    &mut map,
                    GenericItem::Electricity,
                    -energy_usage.amount * 60.0 / 30.0,
                );
            }
            for (key, value) in energy_related_flow.into_iter() {
                index_map_update_entry(&mut map, key, value);
            }
        }
        if let Some(recipe) = data.recipes.get(&self.recipe.0) {
            base_speed /= recipe.energy_required;
            module_effects.productivity = module_effects
                .productivity
                .clamp(0.0, recipe.maximum_productivity);
            module_effects = module_effects.clamped();
            index_map_update_entry(
                &mut map,
                GenericItem::Electricity,
                -self.module_config.get_consumption(data),
            );
            for ingredient in &recipe.ingredients {
                match ingredient {
                    RecipeIngredient::Item(item) => {
                        let key =
                            GenericItem::Item(IdWithQuality(item.name.clone(), self.recipe.1));
                        index_map_update_entry(
                            &mut map,
                            key,
                            -item.amount * (1.0 + module_effects.speed) * base_speed,
                        );
                    }
                    RecipeIngredient::Fluid(fluid) => {
                        let min_temperature = fluid
                            .temperature
                            .or(fluid.minimum_temperature)
                            .map_or(i32::MIN, |t| t as i32);

                        let max_temperature = fluid
                            .temperature
                            .or(fluid.maximum_temperature)
                            .map_or(i32::MAX, |t| t as i32);

                        let key = GenericItem::Fluid {
                            name: fluid.name.clone(),
                            temperature: [min_temperature, max_temperature],
                        };
                        index_map_update_entry(
                            &mut map,
                            key,
                            -fluid.amount * (1.0 + module_effects.speed) * base_speed,
                        );
                    }
                }
            }
            let quality_distribution = calc_quality_distribution(
                &data.qualities,
                module_effects.quality,
                self.recipe.1 as usize,
                proj.max_quality() as usize,
            );
            if is_rocket {
                index_map_update_entry(
                    &mut map,
                    GenericItem::RocketCapacity { stacks, by_weight },
                    1.0 / rocket_parts_required,
                );
            } else {
                for result in &recipe.results {
                    match result {
                        RecipeResult::Item(item) => {
                            let (base_yield, extra_yield) = item.normalized_output();
                            let total_yield = (base_yield
                                + extra_yield
                                    * module_effects
                                        .productivity
                                        .clamp(0.0, recipe.maximum_productivity))
                                * (1.0 + module_effects.speed)
                                * base_speed;

                            for (quality_level, &quality_prob) in
                                quality_distribution.iter().enumerate()
                            {
                                if quality_prob > 0.0 {
                                    let quality_key = GenericItem::Item(IdWithQuality(
                                        item.name.clone(),
                                        quality_level as u8,
                                    ));
                                    index_map_update_entry(
                                        &mut map,
                                        quality_key,
                                        total_yield * quality_prob,
                                    );
                                }
                            }
                        }
                        RecipeResult::Fluid(fluid) => {
                            let default_temperature = fluid
                                .temperature
                                .or(fluid.min_temperature)
                                .or(fluid.max_temperature)
                                .unwrap_or(
                                    data.fluids
                                        .get(&fluid.name)
                                        .as_ref()
                                        .unwrap()
                                        .default_temperature,
                                );
                            let key = GenericItem::Fluid {
                                name: fluid.name.clone(),
                                // temperature: fluid.temperature.map(|x| x as i32),
                                temperature: [
                                    default_temperature as i32,
                                    default_temperature as i32,
                                ],
                            };
                            let (base_yield, extra_yield) = fluid.normalized_output();
                            index_map_update_entry(
                                &mut map,
                                key,
                                (base_yield
                                    + extra_yield
                                        * module_effects
                                            .productivity
                                            .clamp(0.0, recipe.maximum_productivity))
                                    * (1.0 + module_effects.speed)
                                    * base_speed,
                            );
                        }
                    }
                }
            }
        }
        map
    }

    fn cost(&self, data: &DataContext, _proj: &ProjectContext, _factory: &FactoryContext) -> f64 {
        if let Some(crafter) = data.crafters.get(&self.machine.0) {
            crafter
                .base
                .collision_box
                .as_ref()
                .map_or(1.0, |bounding_box| bounding_box.get_area())
        } else {
            16.0
        }
    }
}

#[test]
fn test_recipe_normalized() {
    let data = crate::factorio::DataContext::test_load();
    let proj = crate::factorio::ProjectContext::default();
    let factory = crate::factorio::planner::FactoryContext::default();

    let recipe_config = RecipeInstance {
        recipe: ("iron-gear-wheel".to_string(), 0).into(),
        machine: "assembling-machine-1".into(),
        module_config: ModuleConfig::new(),
        fuel: Some(("nutrients".to_string(), 0).into()),
    };
    let result = recipe_config.as_flow(&data, &proj, &factory);
    println!("Recipe Result: {:?}", result);
    let result_with_location =
        crate::factorio::model::data::make_located_generic_recipe(result.clone(), 1);
    println!("Recipe Result with Location: {:?}", result_with_location);
}

#[serde_as]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct RecipeMechanic {
    #[serde(skip)]
    pub operations: Vec<(usize, EntryOpRequest)>,

    pub instances: Vec<RecipeInstance>,
    #[serde(default)]
    pub machine_preferences: Vec<IdWithQuality>,
    #[serde(default)]
    pub alternative_count: usize,
    #[serde(skip)]
    pub new_machine_preference: Option<IdWithQuality>,

    pub enumerate_modules: Vec<IdWithQuality>,

    #[serde(default)]
    #[serde_with(DefaultOnError)]
    pub enumerate_beacons: Vec<AutoBeaconConfig>,

    #[serde(skip)]
    pub new_enumerate_module: Option<IdWithQuality>,

    #[serde(skip)]
    pub suggestion_item: Option<GenericItem>,
    #[serde(skip)]
    pub suggestion_amount: f64,
    #[serde(skip)]
    pub suggested_recipes: HashSet<String>,
    #[serde(skip)]
    pub selected_suggested_recipe: Option<String>,
    #[serde(skip)]
    pub suggested_recipes_filter: String,
}

// 不排除我会往自动插件塔枚举添加更多功能，先包一层……
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AutoBeaconConfig {
    pub module_config: ModuleConfig,
}

pub fn select_crafter_for_recipe(
    data: &DataContext,
    proj: &ProjectContext,
    factory: &FactoryContext,
    recipe: &RecipePrototype,
    preferences: &[IdWithQuality],
    excluding: &[IdWithQuality],
) -> IdWithQuality {
    // 优先选择用户偏好
    for pref in preferences {
        if let Some(crafter) = data.crafters.get(&pref.0)
            && machine_fits_for_recipe(crafter, recipe)
        {
            return pref.clone();
        }
    }
    let mut measure = 0.0;
    let mut selected = "entity-unknown".to_string();
    fn measure_crafter(crafter: &CraftingMachinePrototype) -> f64 {
        let mut score = crafter.crafting_speed
            / crafter
                .base
                .collision_box
                .as_ref()
                .map_or(25.0, |bb| bb.get_area());
        if let Some(effect_receiver) = &crafter.effect_receiver {
            score *= 1.0 + effect_receiver.base_effect.speed;
            score *= 1.0 + (effect_receiver.base_effect.productivity * 2.0);
        }
        if matches!(crafter.energy_source, EnergySource::Electric(_)) {
            score *= 8.0;
        }
        score *= 1.0 + crafter.module_slots;
        score
    }
    // 找不到用户偏好时，选择最快的机器
    for (crafter_name, crafter) in &data.crafters {
        if machine_fits_for_recipe(crafter, recipe)
            && measure_crafter(crafter) > measure
            && !excluding.iter().any(|ex| ex.0 == *crafter_name)
            && proj.is_prototype_accessible("entity", crafter_name)
        {
            measure = measure_crafter(crafter);
            selected = crafter_name.clone();
        }
    }
    (selected, factory.major_quality).into()
}

impl SolveContext for RecipeMechanic {
    type Game = DataContext;
    type Item = GenericItem;
}

#[typetag::serde(name = "factorio:recipe")]
impl SerdeFactorioMechanic for RecipeMechanic{} impl FactorioMechanic for RecipeMechanic {
    fn name(&self) -> String {
        "配方".to_string()
    }

    fn editor_view(
        &mut self,
        ui: &mut egui::Ui,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        if ui.button("添加配方").clicked() {
            let new_config = RecipeInstance::default();
            self.instances.push(new_config);
            changed = true;
        }
        ui.collapsing("[自动/手动]机器偏好", |ui| {
            let icon = Icon::new(data, "entity", "entity-unknown");
            ui.label("[自动]备选机器数量");
            ui.add(
                egui::DragValue::new(&mut self.alternative_count)
                    .speed(1)
                    .range(1..=3)
                    .clamp_existing_to_range(true),
            );

            let button = ui
                .add_sized([35.0, 35.0], icon)
                .on_hover_text("选择新的机器顺序依据");
            ui.add(
                SelectorModal::new(button.id, data, "选择机器")
                    .with_toggle(button.clicked())
                    .with_selector(
                        Selector::new(data, "entity")
                            .with_output(&mut self.new_machine_preference)
                            .with_filter(|s: &IdWithQuality, f: &DataContext| {
                                f.crafters.contains_key(&s.0)
                                    && proj.is_prototype_accessible("entity", &s.0)
                            }),
                    ),
            );
            if self.new_machine_preference.is_some() {
                let new_machine = self.new_machine_preference.take().unwrap();
                // 移除已有的相同机器
                self.machine_preferences.retain(|m| m.0 != new_machine.0);
                // 插入到最前面
                self.machine_preferences.insert(0, new_machine);
                changed = true;
            }
            ui.separator();
            let mut delete_target = None;
            egui_dnd::dnd(ui, "machine-preferences").show_vec(
                &mut self.machine_preferences,
                |ui, machine, handle, _| {
                    ui.horizontal_top(|ui| {
                        handle.ui_sized(ui, [15.0, 35.0].into(), |ui| {
                            ui.heading("☰");
                        });
                        let icon = Icon::new(data, "entity", &machine.0).with_quality(machine.1);
                        let button = ui.add(icon);
                        if data.crafters.contains_key(&machine.0) {
                            if button.secondary_clicked() {
                                delete_target = Some(machine.clone());
                                changed = true;
                            }
                        } else {
                            delete_target = Some(machine.clone());
                            changed = true;
                        }
                        ui.add(
                            SelectorModal::new(button.id, data, "选择机器")
                                .with_toggle(button.clicked())
                                .with_selector(
                                    Selector::new(data, "entity")
                                        .with_current(machine)
                                        .with_filter(|s: &IdWithQuality, f: &DataContext| {
                                            f.crafters.contains_key(&s.0)
                                                && proj.is_prototype_accessible("entity", &s.0)
                                        }),
                                ),
                        );
                    });
                },
            );
            if let Some(machine) = delete_target {
                self.machine_preferences.retain(|m| m != &machine);
            }
        });
        ui.separator();
        ui.collapsing("[自动]枚举插件", |ui| {
            if ui
                .button("使用最佳插件")
                .on_hover_text("根据当前科技等级，选择每个类别的最佳插件加入枚举。")
                .clicked()
            {
                let mut modules_by_category: HashMap<String, &ModulePrototype> = HashMap::new();
                for module in data.modules.values() {
                    if proj.is_prototype_accessible("item", &module.base.name) {
                        let category = module.category.clone();
                        modules_by_category
                            .entry(category.clone())
                            .and_modify(|m| {
                                if module.tier > m.tier {
                                    *m = module;
                                }
                            })
                            .or_insert(module);
                    }
                }
                self.enumerate_modules = modules_by_category
                    .values()
                    .map(|m| (m.base.name.clone(), factory.major_quality).into())
                    .collect();
            }
            let icon = Icon::new(data, "item", "empty-module-slot");
            let button = ui
                .add_sized([35.0, 35.0], icon)
                .on_hover_text("选择新的枚举插件。修改插件请先删除。");
            ui.add(
                SelectorModal::new(button.id, data, "选择枚举插件")
                    .with_toggle(button.clicked())
                    .with_selector(
                        Selector::new(data, "item")
                            .with_output(&mut self.new_enumerate_module)
                            .with_filter(|item: &IdWithQuality, data: &DataContext| {
                                data.modules.contains_key(&item.0)
                                    && proj.is_prototype_accessible("item", &item.0)
                            }),
                    ),
            );
            if self.new_enumerate_module.is_some() {
                let new_module = self.new_enumerate_module.take().unwrap();
                // 移除已有的相同插件
                self.enumerate_modules.retain(|m| m != &new_module);
                // 插入到最前面
                self.enumerate_modules.insert(0, new_module);
                changed = true;
            }
            // 插件顺序无关，所以不提供相对移动操作
            let mut delele_module = None;
            ui.separator();
            for module in &self.enumerate_modules {
                let button = ui
                    .add_sized(
                        [35.0, 35.0],
                        Icon::new(data, "item", &module.0).with_quality(module.1),
                    )
                    .on_hover_text("无法编辑，右键可删除。");
                if button.secondary_clicked() {
                    delele_module = Some(module.clone());
                    changed = true;
                }
            }
            if let Some(module) = delele_module {
                self.enumerate_modules.retain(|m| m != &module);
            }
        });
        ui.separator();
        ui.collapsing("[自动]插件塔", |ui| {
            ui.label("添加新建筑时，会选取第一个满足条件的插件塔配置。");
            if ui.button("添加插件塔").clicked() {
                self.enumerate_beacons.push(AutoBeaconConfig {
                    module_config: ModuleConfig::new(),
                });
                changed = true;
            }
            self.enumerate_beacons.retain_mut(|config| {
                ui.separator();
                let mut deleted = false;
                if ui.button("删除").clicked() {
                    deleted = true;
                    changed = true;
                }
                ui.add(
                    ModuleConfigEditor::new(
                        data,
                        &mut config.module_config,
                        0,
                        &Some(EffectTypeLimitation::new(true, true, true, true, true)),
                        &None,
                    )
                    .with_edit_modules(false)
                    .with_project_context(proj),
                );
                !deleted
            });
        });

        changed
    }

    fn instances(&self) -> Vec<&dyn AsFlow> {
        self.instances.iter().map(|m| m as &dyn AsFlow).collect()
    }

    fn instance_len(&self) -> usize {
        self.instances.len()
    }

    fn instance_view(
        &mut self,
        idx: usize,
        ui: &mut egui::Ui,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;

        let instance = &mut self.instances[idx];
        ui.vertical(|ui| {
            ui.label("配方");
            let recipe_button = ui.add_sized(
                [35.0, 35.0],
                Icon::new(data, "recipe", &instance.recipe.0).with_quality(instance.recipe.1),
            );
            changed |= ui
                .add(
                    SelectorModal::new(recipe_button.id, data, "选择配方")
                        .with_toggle(recipe_button.clicked())
                        .with_selector(
                            Selector::new(data, "recipe")
                                .with_current(&mut instance.recipe)
                                .with_filter(|s: &IdWithQuality, _f| {
                                    proj.is_prototype_accessible("recipe", &s.0)
                                }),
                        ),
                )
                .changed();
        });
        if changed
            && let Some(recipe) = data.recipes.get(&instance.recipe.0)
            && data
                .crafters
                .get(&instance.machine.0)
                .is_none_or(|crafter| {
                    !machine_fits_for_recipe(crafter, data.recipes.get(&instance.recipe.0).unwrap())
                })
        {
            instance.machine = select_crafter_for_recipe(
                data,
                proj,
                factory,
                recipe,
                &self.machine_preferences,
                &[],
            );
            instance.fuel = None;
            instance.module_config = ModuleConfig::new();
        }
        ui.separator();
        ui.vertical(|ui| {
            ui.label("组装机");
            let entity_button = ui.add_sized(
                [35.0, 35.0],
                Icon::new(data, "entity", &instance.machine.0).with_quality(instance.machine.1),
            );

            let selector = Selector::new(data, "entity")
                .with_filter(|crafter_name: &IdWithQuality, data: &DataContext| {
                    if let Some(crafter) = data.crafters.get(&crafter_name.0)
                        && let Some(recipe_prototype) = data.recipes.get(instance.recipe.0.as_str())
                        && proj.is_prototype_accessible("entity", &crafter_name.0)
                    {
                        return machine_fits_for_recipe(crafter, recipe_prototype);
                    }
                    false
                })
                .with_current(&mut instance.machine);

            let widget = SelectorModal::new(entity_button.id, data, "选择制造设备")
                .with_toggle(entity_button.clicked())
                .with_selector(selector);
            changed |= ui.add(widget).changed();
        });

        ui.separator();

        if let Some(crafter) = data.crafters.get(&instance.machine.0)
            && let Some(recipe) = data.recipes.get(&instance.recipe.0)
        {
            let (allowed_effects, allowed_module_categories) =
                collect_module_limitations(crafter, recipe);

            changed |= ui
                .add(
                    ModuleConfigEditor::new(
                        data,
                        &mut instance.module_config,
                        crafter.module_slots as usize,
                        &Some(allowed_effects),
                        &allowed_module_categories,
                    )
                    .with_project_context(proj),
                )
                .changed();
        };

        changed
    }

    fn instance_operate(
        &mut self,
        idx: usize,
        f: &mut dyn FnMut(&mut dyn AsFlow) -> EntryOpRequest,
    ) {
        let op = f(&mut self.instances[idx] as &mut dyn AsFlow);
        if !matches!(op, EntryOpRequest::None) {
            self.operations.push((idx, op));
        }
    }

    fn submit_operations(&mut self) -> Vec<EntryOpResult> {
        self.instances.update_elements(&mut self.operations)
    }

    fn update_suggestion(
        &mut self,
        data: &DataContext,
        proj: &ProjectContext,
        _factory: &FactoryContext,
        item: &GenericItem,
        amount: f64,
    ) {
        self.suggested_recipes.clear();
        self.suggestion_item = Some(item.clone());
        self.suggestion_amount = amount;
        for recipe_proto in data.recipes.values() {
            if !proj.is_prototype_accessible("recipe", &recipe_proto.base.name) {
                continue;
            }
            match item {
                GenericItem::Item(id_with_quality) => {
                    let mut total_yield = 0.0;
                    for ingredient in &recipe_proto.ingredients {
                        if let RecipeIngredient::Item(item_ingredient) = ingredient
                            && item_ingredient.name == id_with_quality.0
                        {
                            total_yield -= item_ingredient.amount;
                        }
                    }
                    for result in &recipe_proto.results {
                        if let RecipeResult::Item(item_result) = result
                            && item_result.name == id_with_quality.0
                        {
                            total_yield += item_result.normalized_output().0;
                        }
                    }
                    if total_yield * amount < 0.0 {
                        self.suggested_recipes
                            .insert(recipe_proto.base.name.clone());
                    }
                }
                GenericItem::Fluid {
                    name,
                    temperature: _,
                } => {
                    let mut total_yield = 0.0;
                    for ingredient in &recipe_proto.ingredients {
                        if let RecipeIngredient::Fluid(fluid_ingredient) = ingredient
                            && &fluid_ingredient.name == name
                        {
                            total_yield -= fluid_ingredient.amount;
                        }
                    }
                    for result in &recipe_proto.results {
                        if let RecipeResult::Fluid(fluid_result) = result
                            && &fluid_result.name == name
                        {
                            total_yield += fluid_result.normalized_output().0;
                        }
                    }
                    if total_yield * amount < 0.0 {
                        self.suggested_recipes
                            .insert(recipe_proto.base.name.clone());
                    }
                }
                _ => {}
            }
        }
    }

    fn suggestion_view(
        &mut self,
        ui: &mut egui::Ui,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) -> bool {
        let mut changed = false;
        ui.add(egui::TextEdit::singleline(&mut self.suggested_recipes_filter).hint_text("筛选器"));
        ui.add(
            Selector::new(data, "recipe")
                .with_output(&mut self.selected_suggested_recipe)
                .with_filter(|id: &str, data| {
                    self.suggested_recipes.contains(id)
                        && (id
                            .to_lowercase()
                            .contains(&self.suggested_recipes_filter.to_lowercase())
                            || data
                                .get_display_name("recipe", id)
                                .to_lowercase()
                                .contains(&self.suggested_recipes_filter.to_lowercase()))
                }),
        );
        if let Some(recipe) = &self.selected_suggested_recipe {
            let quality = match self.suggestion_item {
                Some(GenericItem::Item(ref id_with_quality)) => id_with_quality.1,
                _ => 0,
            };
            self.instances.push(RecipeInstance {
                recipe: IdWithQuality(recipe.clone(), quality),
                machine: select_crafter_for_recipe(
                    data,
                    proj,
                    factory,
                    data.recipes.get(recipe.as_str()).unwrap(),
                    &self.machine_preferences,
                    &[],
                ),
                ..Default::default()
            });
            self.selected_suggested_recipe = None;
            changed = true;
        }
        changed
    }

    fn auto_populate(
        &mut self,
        data: &DataContext,
        proj: &ProjectContext,
        factory: &FactoryContext,
    ) {
        for (recipe_name, recipe_proto) in &data.recipes {
            if !proj.is_prototype_accessible("recipe", recipe_name) {
                continue;
            }
            if let Some(planet_name) = &factory.planet
                && let Some(planet) = data.planets.get(planet_name)
                && !surface_condition_satisfied(
                    &recipe_proto.surface_conditions,
                    &planet.surface_properties,
                    &data.surface_properties,
                )
            {
                continue;
            }
            let quality_range = if recipe_proto
                .ingredients
                .iter()
                .any(|ingredient| matches!(ingredient, RecipeIngredient::Item(..)))
            {
                proj.max_quality() as usize + 1
            } else {
                1
            };
            let mut machines = vec![];
            for _ in 0..self.alternative_count.clamp(1, 3) {
                let machine_name = select_crafter_for_recipe(
                    data,
                    proj,
                    factory,
                    recipe_proto,
                    &self.machine_preferences,
                    &machines,
                );
                if &machine_name.0 == "entity-unknown" {
                    break;
                }
                machines.push(machine_name);
            }
            machines.iter().for_each(|machine_name| {
                if let Some(machine_proto) = data.crafters.get(&machine_name.0) {
                    if let Some(planet_name) = &factory.planet
                        && let Some(planet) = data.planets.get(planet_name)
                        && !surface_condition_satisfied(
                            &machine_proto.base.surface_conditions,
                            &planet.surface_properties,
                            &data.surface_properties,
                        )
                    {
                        return;
                    }
                    if machine_proto
                        .fixed_recipe
                        .as_ref()
                        .is_some_and(|fixed_recipe| fixed_recipe != recipe_name)
                    {
                        return;
                    }

                    let (allowed_effects, option_allowed_modules) =
                        collect_module_limitations(machine_proto, recipe_proto);
                    let allowed_effects = Some(allowed_effects);
                    let allowed_modules = self
                        .enumerate_modules
                        .clone()
                        .into_iter()
                        .filter(|module_name| {
                            if let Some(module) = data.modules.get(&module_name.0) {
                                option_allowed_modules.as_ref().is_none_or(
                                    |allowed_module_categories| {
                                        allowed_module_categories.contains(&module.category)
                                    },
                                ) && module_effects_allowed(module, &allowed_effects)
                            } else {
                                false
                            }
                        })
                        .collect::<Vec<_>>();
                    for comb in Compositions::new(
                        allowed_modules.len() + 1,
                        machine_proto.module_slots as usize,
                    ) {
                        for quality in 0..quality_range {
                            let mut modules = vec![];
                            for module_id in 0..allowed_modules.len() {
                                for _ in 0..comb[module_id] {
                                    modules.push(allowed_modules[module_id].clone());
                                }
                            }
                            self.instances.push(RecipeInstance {
                                recipe: IdWithQuality(recipe_name.clone(), quality as u8),
                                machine: machine_name.clone(),
                                module_config: ModuleConfig {
                                    modules: modules.clone(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            });
                            for auto_beacon_config in &self.enumerate_beacons {
                                self.instances.push(RecipeInstance {
                                    recipe: IdWithQuality(recipe_name.clone(), quality as u8),
                                    machine: machine_name.clone(),
                                    module_config: ModuleConfig {
                                        modules: modules.clone(),
                                        ..auto_beacon_config.module_config.clone()
                                    },
                                    ..Default::default()
                                });
                            }
                        }
                    }
                }
            });
        }
    }
}

fn collect_module_limitations(
    crafter: &CraftingMachinePrototype,
    recipe: &RecipePrototype,
) -> (EffectTypeLimitation, Option<Vec<String>>) {
    let allowed_effects = EffectTypeLimitation::new(
        recipe.allow_consumption,
        recipe.allow_speed,
        recipe.allow_productivity,
        recipe.allow_pollution,
        recipe.allow_quality,
    )
    .intersect(
        crafter
            .allowed_effects
            .as_ref()
            .unwrap_or(&EffectTypeLimitation::default()),
    );
    let allowed_module_categories = match (
        crafter.allowed_module_categories.as_ref(),
        recipe.allowed_module_categories.as_ref(),
    ) {
        (None, None) => None,
        (None, Some(_)) => recipe.allowed_module_categories.clone(),
        (Some(_), None) => crafter.allowed_module_categories.clone(),
        (Some(a), Some(b)) => Some([a.to_vec().as_slice(), b.to_vec().as_slice()].concat()),
    };
    (allowed_effects, allowed_module_categories)
}
