use std::fmt::Debug;

use crate::{
    concept::*,
    factorio::{
        common::*,
        editor::{
            hover::PrototypeHover,
            icon::{GenericIcon, Icon},
            modal::show_modal,
            selector::{ItemSelector, item_with_quality_selector_modal},
        },
        model::{
            context::{FactorioContext, GenericItem},
            energy::energy_source_as_flow,
            entity::EntityPrototype,
            module::{ModuleConfig, ModuleConfigEditor},
            quality::calc_quality_distribution,
        },
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename = "factorio:recipe")]
pub struct RecipeConfig {
    pub recipe: IdWithQuality,
    pub machine: Option<IdWithQuality>,
    pub module_config: ModuleConfig,

    /// 当机器的能源类型为Fluid、Burner时，用统一的抽象能源还是用具体的燃料
    /// 类型为Electric、Heat、Void时无效
    /// 类型为Fluid时，值为(流体名, 流体温度)
    /// 类型为Burner时，值为(物品名, 物品品质)
    pub instance_fuel: Option<(String, i32)>,
}

impl SolveContext for RecipeConfig {
    type GameContext = FactorioContext;
    type ItemIdentType = GenericItem;
}

impl Default for RecipeConfig {
    fn default() -> Self {
        RecipeConfig {
            recipe: ("recipe-unknown".to_string(), 0).into(),
            machine: None,
            module_config: ModuleConfig::new(),
            instance_fuel: None,
        }
    }
}

impl AsFlow for RecipeConfig {
    fn as_flow(&self, ctx: &FactorioContext) -> Flow<Self::ItemIdentType> {
        let mut map = Flow::new();

        let mut module_effects = self.module_config.get_effect(ctx);

        let mut base_speed = 1.0;

        let crafter = self.machine.as_ref().map(|machine| {
            ctx.crafters
                .get(&machine.0)
                .expect("RecipeConfig 中的机器在上下文中不存在")
        });

        module_effects = module_effects.clamped();

        if let Some(crafter) = crafter {
            module_effects = module_effects
                + crafter
                    .effect_receiver
                    .clone()
                    .unwrap_or_default()
                    .base_effect
                    .clone();
            base_speed = crafter.crafting_speed;
            let quality_level = self
                    .machine
                    .as_ref()
                    .unwrap()
                    .1 as usize;
            if crafter.crafting_speed_quality_multiplier.is_some() {
                
                let quality = &ctx.qualities[quality_level].base.name;
                let speed_multiplier = crafter
                    .crafting_speed_quality_multiplier
                    .as_ref()
                    .unwrap()
                    .get(quality)
                    .cloned()
                    .unwrap_or(1.0);
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

        let recipe = ctx
            .recipes
            .get(&self.recipe.0)
            .expect("RecipeConfig 中的配方在上下文中不存在");

        base_speed /= recipe.energy_required;

        for ingredient in &recipe.ingredients {
            match ingredient {
                RecipeIngredient::Item(item) => {
                    let key = GenericItem::Item {
                        name: item.name.clone(),
                        quality: self.recipe.1,
                    };
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

                    for (quality_level, &quality_prob) in quality_distribution.iter().enumerate() {
                        if quality_prob > 0.0 {
                            let qual_key = GenericItem::Item {
                                name: item.name.clone(),
                                quality: quality_level as u8,
                            };
                            index_map_update_entry(&mut map, qual_key, total_yield * quality_prob);
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

        map
    }

    fn cost(&self, ctx: &Self::GameContext) -> f64 {
        if self.machine.is_some() {
            let crafter = ctx.crafters.get(&self.machine.as_ref().unwrap().0).unwrap();
            crafter
                .base
                .collision_box
                .as_ref()
                .map_or(1.0, |bounding_box| match bounding_box {
                    BoundingBox::Struct {
                        left_top,
                        right_bottom,
                        orientation: _,
                    } => {
                        f64::ceil(right_bottom.1 - left_top.1)
                            * f64::ceil(right_bottom.0 - left_top.0)
                    }
                    BoundingBox::Pair(map_position, map_position1) => {
                        f64::ceil(map_position1.1 - map_position.1)
                            * f64::ceil(map_position1.0 - map_position.0)
                    }
                    BoundingBox::Triplet(map_position, map_position1, _) => {
                        f64::ceil(map_position1.1 - map_position.1)
                            * f64::ceil(map_position1.0 - map_position.0)
                    }
                })
        } else {
            16.0
        }
    }
}

#[test]
fn test_recipe_normalized() {
    let ctx = FactorioContext::load(
        &serde_json::from_str(include_str!("../../../assets/data-raw-dump.json")).unwrap(),
    );
    let recipe_config = RecipeConfig {
        recipe: ("iron-gear-wheel".to_string(), 0).into(),
        machine: Some(("assembling-machine-1".to_string(), 0).into()),
        module_config: ModuleConfig::new(),
        instance_fuel: Some(("nutrients".to_string(), 0).into()),
    };
    let result = recipe_config.as_flow(&ctx);
    println!("Recipe Result: {:?}", result);
    let result_with_location =
        crate::factorio::model::context::make_located_generic_recipe(result.clone(), 1);
    println!("Recipe Result with Location: {:?}", result_with_location);
}

impl EditorView for RecipeConfig {
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &Self::GameContext) {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.label("配方");

                let recipe_button = ui
                    .add_sized(
                        [35.0, 35.0],
                        Icon {
                            ctx,
                            type_name: "recipe",
                            item_name: &self.recipe.0,
                            quality: self.recipe.1,
                            size: 32.0,
                        },
                    )
                    .interact(egui::Sense::click())
                    .on_hover_ui(|ui| {
                        ui.add(PrototypeHover {
                            ctx,
                            prototype: ctx.recipes.get(&self.recipe.0).unwrap(),
                        });
                    });
                let (selected_id, selected_quality) =
                    item_with_quality_selector_modal(ui, ctx, "选择配方", "recipe", &recipe_button);
                if let Some(selected_id) = selected_id {
                    self.recipe.0 = selected_id;
                }
                if let Some(selected_quality) = selected_quality {
                    self.recipe.1 = selected_quality;
                }
            });
            ui.separator();
            ui.vertical(|ui| {
                let entity_button = if let Some(machine) = &mut self.machine {
                    ui.label("机器");
                    ui.add_sized(
                        [35.0, 35.0],
                        GenericIcon {
                            ctx,
                            item: &GenericItem::Entity {
                                name: machine.0.clone(),
                                quality: machine.1,
                            },
                            size: 32.0,
                        },
                    )
                    .interact(egui::Sense::click())
                } else {
                    ui.label("机器");
                    ui.add_sized([35.0, 35.0], egui::Label::new("空"))
                };

                let mut selected_entity: Option<String> = None;
                show_modal(entity_button.id, entity_button.clicked(), ui, |ui| {
                    ui.label("选择机器");
                    egui::ScrollArea::vertical()
                        .max_width(f32::INFINITY)
                        .auto_shrink(false)
                        .show(ui, |ui| {
                            ui.add(
                                ItemSelector::new(ctx, "entity", &mut selected_entity).with_filter(
                                    |crafter_name, ctx| {
                                        let recipe_prototype =
                                            ctx.recipes.get(self.recipe.0.as_str()).unwrap();
                                        if let Some(crafter) = ctx.crafters.get(crafter_name) {
                                            if crafter.crafting_categories.contains(
                                                recipe_prototype
                                                    .category
                                                    .as_ref()
                                                    .map_or(&"crafting".to_string(), |v| v),
                                            ) {
                                                return true;
                                            }
                                            if recipe_prototype.additional_categories.iter().any(
                                                |cat| crafter.crafting_categories.contains(cat),
                                            ) {
                                                return true;
                                            }
                                        }
                                        false
                                    },
                                ),
                            );
                        });

                    if selected_entity.is_some() {
                        ui.close();
                    }
                });

                if let Some(selected_entity) = selected_entity {
                    self.machine =
                        Some((selected_entity, self.machine.as_ref().map_or(0, |m| m.1)).into());
                }
            });

            ui.separator();
            // TODO: 插件编辑界面

            if let Some(machine_proto) = self
                .machine
                .as_ref()
                .and_then(|machine| ctx.crafters.get(&machine.0))
                && let Some(recipe_proto) = ctx.recipes.get(&self.recipe.0)
            {
                let allowed_effects = EffectTypeLimitation::new(
                    recipe_proto.allow_consumption,
                    recipe_proto.allow_speed,
                    recipe_proto.allow_productivity,
                    recipe_proto.allow_pollution,
                    recipe_proto.allow_quality,
                )
                .intersect(
                    machine_proto
                        .allowed_effects
                        .as_ref()
                        .unwrap_or(&EffectTypeLimitation::default()),
                );
                let allowed_module_categories = match (
                    machine_proto.allowed_module_categories.as_ref(),
                    recipe_proto.allowed_module_categories.as_ref(),
                ) {
                    (None, None) => &None,
                    (None, Some(_)) => &recipe_proto.allowed_module_categories,
                    (Some(_), None) => &machine_proto.allowed_module_categories,
                    (Some(a), Some(b)) => {
                        &Some([a.to_vec().as_slice(), b.to_vec().as_slice()].concat())
                    }
                };

                ui.add(ModuleConfigEditor::new(
                    ctx,
                    &mut self.module_config,
                    machine_proto.module_slots as usize,
                    &Some(allowed_effects),
                    allowed_module_categories,
                ));
            };
        });
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RecipeConfigProvider {
    pub editing: RecipeConfig,
    #[serde(skip)]
    pub sender: MechanicSender<GenericItem, FactorioContext>,
}

impl SolveContext for RecipeConfigProvider {
    type GameContext = FactorioContext;
    type ItemIdentType = GenericItem;
}

impl MechanicProvider for RecipeConfigProvider {
    fn set_mechanic_sender(&mut self, sender: MechanicSender<GenericItem, FactorioContext>) {
        self.sender = sender;
    }

    fn hint_populate(
        &self,
        ctx: &Self::GameContext,
        item: &Self::ItemIdentType,
        value: f64,
    ) -> Vec<Box<dyn Mechanic<ItemIdentType = Self::ItemIdentType, GameContext = Self::GameContext>>>
    {
        let item_name = match item {
            GenericItem::Item { name, .. } => name,
            GenericItem::Fluid { name, .. } => name,
            _ => return vec![], // Not an item or fluid, do nothing.
        };
        let quality = match item {
            GenericItem::Item { quality, .. } => *quality,
            _ => 0,
        };

        let mut suggestions = Vec::new();

        for recipe_proto in ctx.recipes.values() {
            let matches = if recipe_proto.base.hidden {
                false
            } else if value < 0.0 {
                // We have a deficit, need recipes that PRODUCE this item
                recipe_proto.results.iter().any(|result| match result {
                    RecipeResult::Item(r) => &r.name == item_name,
                    RecipeResult::Fluid(r) => &r.name == item_name,
                })
            } else {
                // We have a surplus, need recipes that CONSUME this item
                recipe_proto
                    .ingredients
                    .iter()
                    .any(|ingredient| match ingredient {
                        RecipeIngredient::Item(i) => &i.name == item_name,
                        RecipeIngredient::Fluid(i) => &i.name == item_name,
                    })
            };

            if matches {
                let mut recipe_config = RecipeConfig {
                    recipe: (recipe_proto.base.name.clone(), quality).into(),
                    ..Default::default()
                };
                // Try to find a suitable machine
                let category = recipe_proto
                    .category
                    .as_ref()
                    .map_or("crafting", |s| s.as_str());
                if let Some(machine) = ctx
                    .crafters
                    .values()
                    .find(|crafter| crafter.crafting_categories.contains(&category.to_string()))
                {
                    recipe_config.machine = Some((machine.base.base.name.clone(), 0).into());
                }
                let actual_produce = recipe_config.as_flow(ctx).get(item).cloned().unwrap_or(0.0);
                if (value < 0.0 && actual_produce <= 0.0) || (value > 0.0 && actual_produce >= 0.0)
                {
                    // This recipe does not actually help with the deficit/surplus
                    continue;
                }
                suggestions.push(Box::new(recipe_config)
                    as Box<
                        dyn Mechanic<
                                ItemIdentType = Self::ItemIdentType,
                                GameContext = Self::GameContext,
                            >,
                    >);
            }
        }

        suggestions
    }
}

impl EditorView for RecipeConfigProvider {
    fn editor_view(&mut self, ui: &mut egui::Ui, _ctx: &Self::GameContext) {
        if ui.button("添加配方").clicked() {
            let mut new_config = self.editing.clone();
            new_config.recipe = ("recipe-unknown".to_string(), 0).into();
            self.sender
                .send(Box::new(new_config))
                .expect("RecipeConfigSource 发送配方失败");
        }
    }
}

crate::impl_register_deserializer!(
    for RecipeConfig
    as "factorio:recipe"
    => dyn Mechanic<ItemIdentType = GenericItem, GameContext = FactorioContext>
);
