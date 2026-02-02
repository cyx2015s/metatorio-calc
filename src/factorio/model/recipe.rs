use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
};

use crate::{
    concept::*,
    factorio::{
        common::*,
        editor::{hover::PrototypeHover, icon::Icon},
        modal::SelectorModal,
        model::{
            context::{FactorioContext, GenericItem},
            energy::energy_source_as_flow,
            entity::EntityPrototype,
            module::{ModuleConfig, ModuleConfigEditor},
            quality::calc_quality_distribution,
        },
        selector::Selector,
    },
};

use crate::factorio::common::{as_vec_or_empty, option_as_vec_or_empty};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub struct RecipePrototype {
    #[serde(flatten)]
    pub base: PrototypeBase,

    category: Option<String>,
    #[serde(deserialize_with = "as_vec_or_empty")]
    additional_categories: Vec<String>,

    #[serde(deserialize_with = "as_vec_or_empty")]
    #[serde(default)]
    pub ingredients: Vec<RecipeIngredient>,

    #[serde(deserialize_with = "as_vec_or_empty")]
    #[serde(default)]
    pub results: Vec<RecipeResult>,
    pub main_product: Option<String>,

    #[serde(deserialize_with = "option_as_vec_or_empty")]
    #[serde(default)]
    pub allowed_module_categories: Option<Vec<String>>,

    /// 制作时间（秒）
    pub energy_required: f64,

    /// 配方污染倍数
    pub emissions_multiplier: f64,

    /// 最大产能加成
    pub maximum_productivity: f64,

    /// 开局是否可用
    pub enabled: bool,

    /// 产物若为可变质，是否永远新鲜
    pub result_is_always_fresh: bool,

    /// 是否允许使用降低能耗的插件
    pub allow_consumption: bool,

    /// 是否允许使用增加速度的插件
    pub allow_speed: bool,

    /// 是否允许使用增加产能的插件
    pub allow_productivity: bool,

    /// 是否允许使用降低污染的插件
    pub allow_pollution: bool,

    /// 是否允许使用增加品质的插件
    pub allow_quality: bool,
}

impl Default for RecipePrototype {
    fn default() -> Self {
        RecipePrototype {
            base: PrototypeBase {
                r#type: "recipe".to_string(),
                name: "recipe-unknown".to_string(),
                order: String::new(),
                subgroup: String::new(),
                hidden: false,
                parameter: false,
            },
            main_product: None,
            category: None,
            additional_categories: Vec::new(),
            ingredients: Vec::new(),
            results: Vec::new(),
            allowed_module_categories: None,
            energy_required: 0.5,
            emissions_multiplier: 1.0,
            maximum_productivity: 3.0,
            enabled: true,
            result_is_always_fresh: false,
            allow_consumption: true,
            allow_speed: true,
            allow_productivity: false,
            allow_pollution: true,
            allow_quality: true,
        }
    }
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
    pub min_temperature: Option<f64>,
    pub max_temperature: Option<f64>,
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

    #[serde(deserialize_with = "as_vec_or_empty")]
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

    #[serde(deserialize_with = "option_as_vec_or_empty")]
    #[serde(default)]
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
#[serde(tag = "type", rename = "factorio:recipe")]
pub struct RecipeMechanicInstance {
    pub recipe: IdWithQuality,
    pub machine: IdWithQuality,
    pub module_config: ModuleConfig,

    /// 当机器的能源类型为Fluid、Burner时，用统一的抽象能源还是用具体的燃料
    /// 类型为Electric、Heat、Void时无效
    /// 类型为Fluid时，值为(流体名, 流体温度)
    /// 类型为Burner时，值为(物品名, 物品品质)
    pub instance_fuel: Option<(String, i32)>,
}

impl SolveContext for RecipeMechanicInstance {
    type GameContext = FactorioContext;
    type ItemIdentType = GenericItem;
}

impl Default for RecipeMechanicInstance {
    fn default() -> Self {
        RecipeMechanicInstance {
            recipe: ("recipe-unknown".to_string(), 0).into(),
            machine: ("entity-unknown".to_string(), 0).into(),
            module_config: ModuleConfig::new(),
            instance_fuel: None,
        }
    }
}

impl AsFlow for RecipeMechanicInstance {
    fn as_flow(&self, ctx: &FactorioContext) -> Flow<Self::ItemIdentType> {
        let mut map = Flow::new();

        let mut module_effects = self.module_config.get_effect(ctx).clamped();

        let mut base_speed = 1.0;

        let crafter = ctx.crafters.get(&self.machine.0);

        if let Some(crafter) = crafter {
            module_effects = module_effects
                + crafter
                    .effect_receiver
                    .clone()
                    .unwrap_or_default()
                    .base_effect
                    .clone();
            base_speed = crafter.crafting_speed;
            let quality_level = self.machine.1 as usize;
            if let Some(multiplier) = &crafter.crafting_speed_quality_multiplier {
                let quality = &ctx.qualities[quality_level].base.name;
                let speed_multiplier = multiplier.get(quality).cloned().unwrap_or(1.0);
                base_speed *= speed_multiplier;
            } else {
                let quality = &ctx.qualities[quality_level];
                base_speed *= quality.crafting_machine_speed_multiplier();
            }
            let energy_related_flow = energy_source_as_flow(
                ctx,
                &crafter.energy_source,
                crafter
                    .energy_usage
                    .as_ref()
                    .expect("CraftingMachinePrototype 中的机器没有能量消耗"),
                &module_effects,
                &self.instance_fuel,
                &mut base_speed,
            );
            for (key, value) in energy_related_flow.into_iter() {
                index_map_update_entry(&mut map, key, value);
            }
        }

        if let Some(recipe) = ctx.recipes.get(&self.recipe.0) {
            base_speed /= recipe.energy_required;

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
                        let key = GenericItem::Fluid {
                            name: fluid.name.clone(),
                            temperature: fluid.temperature.map(|x| x as i32),
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
                &ctx.qualities,
                module_effects.quality,
                self.recipe.1 as usize,
                ctx.qualities.len(),
            );
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
                        let key = GenericItem::Fluid {
                            name: fluid.name.clone(),
                            temperature: fluid.temperature.map(|x| x as i32),
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
        map
    }

    fn cost(&self, ctx: &Self::GameContext) -> f64 {
        if let Some(crafter) = ctx.crafters.get(&self.machine.0) {
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
    let ctx = FactorioContext::test_load();
    let recipe_config = RecipeMechanicInstance {
        recipe: ("iron-gear-wheel".to_string(), 0).into(),
        machine: "assembling-machine-1".into(),
        module_config: ModuleConfig::new(),
        instance_fuel: Some(("nutrients".to_string(), 0).into()),
    };
    let result = recipe_config.as_flow(&ctx);
    println!("Recipe Result: {:?}", result);
    let result_with_location =
        crate::factorio::model::context::make_located_generic_recipe(result.clone(), 1);
    println!("Recipe Result with Location: {:?}", result_with_location);
}

impl EditorView for RecipeMechanicInstance {
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &Self::GameContext) -> bool {
        let mut changed = false;

        ui.vertical(|ui| {
            ui.label("配方");

            let recipe_button = ui
                .add_sized(
                    [35.0, 35.0],
                    Icon::new(ctx, "recipe", &self.recipe.0).with_quality(self.recipe.1),
                )
                .interact(egui::Sense::click())
                .on_hover_ui(|ui| {
                    ui.add(PrototypeHover::new(
                        ctx,
                        ctx.recipes.get(&self.recipe.0).unwrap(),
                    ));
                });
            changed |= ui
                .add(
                    SelectorModal::new(recipe_button.id, ctx, "选择配方")
                        .with_toggle(recipe_button.clicked())
                        .with_selector(
                            Selector::new(ctx, "recipe")
                                .with_current(&mut self.recipe)
                                .with_hover(|ui, name: &IdWithQuality, ctx: &FactorioContext| {
                                    if let Some(prototype) = ctx.recipes.get(name.0.as_str()) {
                                        ui.add(PrototypeHover::new(ctx, prototype));
                                    } else {
                                        ui.label(format!("未知配方: {}", name.0));
                                    }
                                }),
                        ),
                )
                .changed();
        });
        if changed {
            // TODO 读取用户设定的偏好
            if let Some(crafter) = ctx.crafters.get(&self.machine.0)
                && !machine_fits_for_recipe(crafter, ctx.recipes.get(&self.recipe.0).unwrap())
            {
                self.machine = "entity-unknown".into();
                self.instance_fuel = None;
                self.module_config = ModuleConfig::new();
            }
        }
        ui.separator();
        ui.vertical(|ui| {
            ui.add_sized([35.0, 15.0], egui::Label::new("机器"));
            let mut entity_button = ui
                .add_sized(
                    [35.0, 35.0],
                    Icon::new(ctx, "entity", &self.machine.0).with_quality(self.machine.1),
                )
                .interact(egui::Sense::click());

            if let Some(crafter) = ctx.crafters.get(&self.machine.0) {
                entity_button = entity_button.on_hover_ui(|ui| {
                    ui.add(PrototypeHover::new(ctx, crafter));
                });
            }

            let recipe_prototype = ctx.recipes.get(self.recipe.0.as_str()).unwrap();
            let selector = Selector::new(ctx, "entity")
                .with_filter(|crafter_name: &IdWithQuality, ctx: &FactorioContext| {
                    if let Some(crafter) = ctx.crafters.get(&crafter_name.0) {
                        return machine_fits_for_recipe(crafter, recipe_prototype);
                    }
                    false
                })
                .with_current(&mut self.machine)
                .with_hover(|ui, name, ctx| {
                    ui.add(PrototypeHover::new(ctx, &ctx.crafters[&name.0]));
                });

            let widget = SelectorModal::new(entity_button.id, ctx, "选择制造设备")
                .with_toggle(entity_button.clicked())
                .with_selector(selector);
            changed |= ui.add(widget).changed();
        });

        ui.separator();

        if let Some(crafter) = ctx.crafters.get(&self.machine.0)
            && let Some(recipe) = ctx.recipes.get(&self.recipe.0)
        {
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
                (None, None) => &None,
                (None, Some(_)) => &recipe.allowed_module_categories,
                (Some(_), None) => &crafter.allowed_module_categories,
                (Some(a), Some(b)) => {
                    &Some([a.to_vec().as_slice(), b.to_vec().as_slice()].concat())
                }
            };

            changed |= ui
                .add(ModuleConfigEditor::new(
                    ctx,
                    &mut self.module_config,
                    crafter.module_slots as usize,
                    &Some(allowed_effects),
                    allowed_module_categories,
                ))
                .changed();
        };

        changed
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename = "factorio:recipe", default)]
#[derive(Default)]
pub struct RecipeMechanic {
    #[serde(skip)]
    pub operations: HashMap<usize, EntryOperation>,

    pub instances: Vec<RecipeMechanicInstance>,

    pub machine_preferences: Vec<IdWithQuality>,

    #[serde(skip)]
    pub new_machine_preference: Option<IdWithQuality>,

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

pub fn select_crafter_for_recipe(
    ctx: &FactorioContext,
    recipe: &RecipePrototype,
    preferences: &[IdWithQuality],
) -> IdWithQuality {
    // 优先选择用户偏好
    for pref in preferences {
        if let Some(crafter) = ctx.crafters.get(&pref.0)
            && machine_fits_for_recipe(crafter, recipe) {
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
        score *= 1.0 + crafter.module_slots / 4.0 ;
        score
    }
    // 找不到用户偏好时，选择最快的机器
    for (crafter_name, crafter) in &ctx.crafters {
        if machine_fits_for_recipe(crafter, recipe) && measure_crafter(crafter) > measure {
            measure = measure_crafter(crafter);
            selected = crafter_name.clone();
        }
    }
    selected.into()
}

impl SolveContext for RecipeMechanic {
    type GameContext = FactorioContext;
    type ItemIdentType = GenericItem;
}

impl Mechanic<FactorioContext, GenericItem> for RecipeMechanic {
    fn name(&self) -> String {
        "配方".to_string()
    }

    fn instances(&self) -> Vec<&AsFactorioFlow> {
        self.instances
            .iter()
            .map(|m| m as &AsFactorioFlow)
            .collect()
    }

    fn instance_len(&self) -> usize {
        self.instances.len()
    }

    fn instance_view(&mut self, idx: usize, ui: &mut egui::Ui, ctx: &FactorioContext) -> bool {
        let mut changed = false;

        let instance = self.instances.get_mut(idx).unwrap();
        ui.vertical(|ui| {
            ui.label("配方");
            let recipe_button = ui
                .add_sized(
                    [35.0, 35.0],
                    Icon::new(ctx, "recipe", &instance.recipe.0).with_quality(instance.recipe.1),
                )
                .interact(egui::Sense::click())
                .on_hover_ui(|ui| {
                    ui.add(PrototypeHover::new(
                        ctx,
                        ctx.recipes.get(&instance.recipe.0).unwrap(),
                    ));
                });
            changed |= ui
                .add(
                    SelectorModal::new(recipe_button.id, ctx, "选择配方")
                        .with_toggle(recipe_button.clicked())
                        .with_selector(
                            Selector::new(ctx, "recipe")
                                .with_current(&mut instance.recipe)
                                .with_hover(|ui, name: &IdWithQuality, ctx: &FactorioContext| {
                                    if let Some(prototype) = ctx.recipes.get(name.0.as_str()) {
                                        ui.add(PrototypeHover::new(ctx, prototype));
                                    } else {
                                        ui.label(format!("未知配方: {}", name.0));
                                    }
                                }),
                        ),
                )
                .changed();
        });
        if changed
            && let Some(recipe) = ctx.recipes.get(&instance.recipe.0)
                && ctx.crafters.get(&instance.machine.0).is_none_or(|crafter| {
                    !machine_fits_for_recipe(crafter, ctx.recipes.get(&instance.recipe.0).unwrap())
                }) {
                    instance.machine =
                        select_crafter_for_recipe(ctx, recipe, &self.machine_preferences);
                    instance.instance_fuel = None;
                    instance.module_config = ModuleConfig::new();
                }
        ui.separator();
        ui.vertical(|ui| {
            ui.add_sized([35.0, 15.0], egui::Label::new("机器"));
            let mut entity_button = ui
                .add_sized(
                    [35.0, 35.0],
                    Icon::new(ctx, "entity", &instance.machine.0).with_quality(instance.machine.1),
                )
                .interact(egui::Sense::click());

            if let Some(crafter) = ctx.crafters.get(&instance.machine.0) {
                entity_button = entity_button.on_hover_ui(|ui| {
                    ui.add(PrototypeHover::new(ctx, crafter));
                });
            }

            let recipe_prototype = ctx.recipes.get(instance.recipe.0.as_str()).unwrap();
            let selector = Selector::new(ctx, "entity")
                .with_filter(|crafter_name: &IdWithQuality, ctx: &FactorioContext| {
                    if let Some(crafter) = ctx.crafters.get(&crafter_name.0) {
                        return machine_fits_for_recipe(crafter, recipe_prototype);
                    }
                    false
                })
                .with_current(&mut instance.machine)
                .with_hover(|ui, name, ctx| {
                    ui.add(PrototypeHover::new(ctx, &ctx.crafters[&name.0]));
                });

            let widget = SelectorModal::new(entity_button.id, ctx, "选择制造设备")
                .with_toggle(entity_button.clicked())
                .with_selector(selector);
            changed |= ui.add(widget).changed();
        });

        ui.separator();

        if let Some(crafter) = ctx.crafters.get(&instance.machine.0)
            && let Some(recipe) = ctx.recipes.get(&instance.recipe.0)
        {
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
                (None, None) => &None,
                (None, Some(_)) => &recipe.allowed_module_categories,
                (Some(_), None) => &crafter.allowed_module_categories,
                (Some(a), Some(b)) => {
                    &Some([a.to_vec().as_slice(), b.to_vec().as_slice()].concat())
                }
            };

            changed |= ui
                .add(ModuleConfigEditor::new(
                    ctx,
                    &mut instance.module_config,
                    crafter.module_slots as usize,
                    &Some(allowed_effects),
                    allowed_module_categories,
                ))
                .changed();
        };

        changed
    }

    fn instance_operate(
        &mut self,
        idx: usize,
        f: &mut dyn FnMut(&mut AsFactorioFlow) -> EntryOperation,
    ) {
        let op = f(&mut self.instances[idx] as &mut AsFactorioFlow);
        if !matches!(op, EntryOperation::None) {
            self.operations.insert(idx, op);
        }
    }

    fn update_suggestion(&mut self, ctx: &FactorioContext, item: &GenericItem, amount: f64) {
        self.suggested_recipes.clear();
        self.suggestion_item = Some(item.clone());
        self.suggestion_amount = amount;
        for recipe_proto in ctx.recipes.values() {
            match item {
                GenericItem::Item(id_with_quality) => {
                    let mut total_yield = 0.0;
                    for ingredient in &recipe_proto.ingredients {
                        if let RecipeIngredient::Item(item_ingredient) = ingredient
                            && item_ingredient.name == id_with_quality.0 {
                                total_yield -= item_ingredient.amount;
                            }
                    }
                    for result in &recipe_proto.results {
                        if let RecipeResult::Item(item_result) = result
                            && item_result.name == id_with_quality.0 {
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
                            && &fluid_ingredient.name == name {
                                total_yield -= fluid_ingredient.amount;
                            }
                    }
                    for result in &recipe_proto.results {
                        if let RecipeResult::Fluid(fluid_result) = result
                            && &fluid_result.name == name {
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

    fn auto_populate(
        &mut self,
        ctx: &FactorioContext,
        sender: AsFlowSender<FactorioContext, GenericItem>, // 传递的所有物品流信息
    ) {
        let _ = ctx;
        let _ = sender;
    }

    fn suggestion_view(&mut self, ui: &mut egui::Ui, ctx: &FactorioContext) -> bool {
        let mut changed = false;
        ui.add(egui::TextEdit::singleline(&mut self.suggested_recipes_filter).hint_text("筛选器"));
        ui.add(
            Selector::new(ctx, "recipe")
                .with_output(&mut self.selected_suggested_recipe)
                .with_filter(|id: &str, ctx| {
                    self.suggested_recipes.contains(id)
                        && (id
                            .to_lowercase()
                            .contains(&self.suggested_recipes_filter.to_lowercase())
                            || ctx
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
            self.instances.push(RecipeMechanicInstance {
                recipe: IdWithQuality(recipe.clone(), quality),
                machine: select_crafter_for_recipe(
                    ctx,
                    ctx.recipes.get(recipe.as_str()).unwrap(),
                    &self.machine_preferences,
                ),
                ..Default::default()
            });
            self.selected_suggested_recipe = None;
            changed = true;
        }
        changed
    }
}

impl EditorView for RecipeMechanic {
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &FactorioContext) -> bool {
        let mut changed = false;
        for idx in 0..self.instances.len() {
            if self
                .operations
                .get(&idx)
                .is_some_and(|v| matches!(v, EntryOperation::Clone))
            {
                self.instances.push(self.instances[idx].clone());
                changed = true;
            }
        }
        for idx in (0..self.instances.len()).rev() {
            if self
                .operations
                .get(&idx)
                .is_some_and(|v| matches!(v, EntryOperation::Drop))
            {
                self.instances.remove(idx);
                changed = true;
            }
        }
        self.operations.clear();
        ui.collapsing("机器偏好", |ui| {
            let icon = Icon::new(ctx, "entity", "entity-unknown");
            let button = ui
                .add(icon)
                .on_hover_text("选择新的机器顺序依据")
                .interact(egui::Sense::click());
            ui.add(
                SelectorModal::new(button.id, ctx, "选择机器")
                    .with_toggle(button.clicked())
                    .with_selector(
                        Selector::new(ctx, "entity")
                            .with_output(&mut self.new_machine_preference)
                            .with_filter(|s: &IdWithQuality, f: &FactorioContext| {
                                f.crafters.contains_key(&s.0)
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
            let mut move_ups = vec![];
            let mut deletes = vec![];
            let inital_len = self.machine_preferences.len();
            for (idx, machine) in self.machine_preferences.iter_mut().enumerate() {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        if ui
                            .add_enabled(idx > 0, egui::Button::new("↑").small())
                            .clicked()
                        {
                            move_ups.push(idx);
                            changed = true;
                        }
                        if ui
                            .add_enabled(idx + 1 < inital_len, egui::Button::new("↓").small())
                            .clicked()
                        {
                            move_ups.push(idx + 1);
                            changed = true;
                        }
                    });

                    let icon = Icon::new(ctx, "entity", &machine.0).with_quality(machine.1);
                    let mut button = ui.add(icon).interact(egui::Sense::click());
                    if let Some(crafter) = ctx.crafters.get(&machine.0) {
                        button = button.on_hover_ui(|ui| {
                            ui.add(PrototypeHover::new(ctx, crafter));
                        });

                        if button.secondary_clicked() {
                            deletes.push(idx);
                            changed = true;
                        }
                    } else {
                        deletes.push(idx);
                    }
                    ui.add(
                        SelectorModal::new(button.id, ctx, "选择机器")
                            .with_toggle(button.clicked())
                            .with_selector(
                                Selector::new(ctx, "entity")
                                    .with_current(machine)
                                    .with_filter(|s: &IdWithQuality, f: &FactorioContext| {
                                        f.crafters.contains_key(&s.0)
                                    }),
                            ),
                    );
                });
            }
            for idx in move_ups {
                self.machine_preferences.swap(idx, idx - 1);
            }
            for idx in deletes.iter().rev() {
                self.machine_preferences.remove(*idx);
            }
        });

        if ui.button("添加配方").clicked() {
            let new_config = RecipeMechanicInstance::default();
            self.instances.push(new_config);
            changed = true;
        }

        changed
    }
}

crate::impl_register_deserializer!(
    for RecipeMechanic
    as "factorio:recipe"
    => dyn Mechanic<FactorioContext, GenericItem>
);
