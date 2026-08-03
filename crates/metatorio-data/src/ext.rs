//! Phase 4 方向 A：生成组件的扩展方法。
//!
//! 第一阶段：翻译生成代码中带 `默认(schema):` 注释的字段——这些默认值
//! 无法由 codegen 推断（非字面量、依赖其他字段或外部数据），只在注释中记录。
//!
//! 方法约定：**与字段同名**的方法返回该字段的**生效值**（effective value）——
//! 字段已设置则返回字段值，未设置（None）时回落到 schema 注释中的默认值。
//! 返回类型按字段形态分三档：
//! - Copy 标量/容器（f64/bool/u16/Color/Vector 等）→ `Option<T>` 值返回；
//! - 非 Copy 结构且默认值总是有语义（如 `EffectValueRange`/`BoundingBox`）→ `&T`，
//!   默认值用 `static`/`const` 提升为 `&'static`（含 Vec 的 Drop 类型必须 `static`，
//!   纯字面量可 `const`）；
//! - `Vec<T>` 字段 → `&[T]`（`as_deref()`）；默认值本身为 None 且 None 语义真实
//!   （如 `autoplace`=不可自动放置、`allowed_module_categories`=全部允许）→ 保留 `Option`。
//!
//! 需要组件外数据（name、其他组件、UtilityConstants 等）的默认值暂缺，以
//! `TODO(defaults):` 注释标注。

use std::sync::LazyLock;

use crate::types::{
    BoundingBox, Color, EffectTypeLimitation, EffectValueRangeOpt, EnergyAmount, Vector,
};

pub use crate::generated_components::*;

/// `{r=1, g=1, b=1, a=1}` 白色
const WHITE: Color = Color(255, 255, 255, 255);

/// 空碰撞盒（`{{0, 0}, {0, 0}}`，无碰撞）
const EMPTY_BOUNDING_BOX: BoundingBox = BoundingBox(Vector(0.0, 0.0), Vector(0.0, 0.0));

/// 默认(schema): No effects are allowed（含 Vec 有 Drop，const 不可提升为 `&'static`，用 static）
const NO_EFFECTS: EffectTypeLimitation = EffectTypeLimitation {
    allowed: [false; 5],
};

/// 默认(schema): All effects except quality are allowed
const LAB_EFFECTS: EffectTypeLimitation = EffectTypeLimitation {
    allowed: [true, true, true, true, false],
};

/// 默认(schema): All effects are allowed
const MINING_EFFECTS: EffectTypeLimitation = EffectTypeLimitation { allowed: [true; 5] };

/// 默认(schema): `{"crafting"}`（String 元素不可 const，惰性初始化）
static CRAFTING_CATEGORIES: LazyLock<Vec<String>> = LazyLock::new(|| vec!["crafting".to_string()]);

// ── EffectReceiver ────────────────────────────────────────────────

pub struct EffectValueRange {
    pub low: f64,
    pub high: f64,
}

const DEFAULT_EFFECT_VALUE_RANGE: EffectValueRange = EffectValueRange {
    low: -0.8,
    high: 1000.0,
};

const DEFAULT_QUALITY_VALUE_RANGE: EffectValueRange = EffectValueRange {
    low: 0.0,
    high: 1000.0,
};
trait EffectValueRangeExt {
    fn update(&self, other: EffectValueRange) -> EffectValueRange;
}
impl EffectValueRangeExt for Option<EffectValueRangeOpt> {
    fn update(&self, other: EffectValueRange) -> EffectValueRange {
        self.as_ref()
            .map(|r| EffectValueRange {
                low: r.low.unwrap_or(other.low),
                high: r.high.unwrap_or(other.high),
            })
            .unwrap_or(other)
    }
}

impl EffectReceiver {
    /// 默认(schema): `{ low = -0.8, high = 1000 }`
    pub fn consumption_limits(&self) -> EffectValueRange {
        self.consumption_limits.update(DEFAULT_EFFECT_VALUE_RANGE)
    }

    /// 默认(schema): `{ low = -0.8, high = 1000 }`
    pub fn pollution_limits(&self) -> EffectValueRange {
        self.pollution_limits.update(DEFAULT_EFFECT_VALUE_RANGE)
    }

    /// 默认(schema): `{ low = -0.8, high = 1000 }`
    pub fn productivity_limits(&self) -> EffectValueRange {
        self.productivity_limits.update(DEFAULT_EFFECT_VALUE_RANGE)
    }

    /// 默认(schema): `{ low = 0, high = 1000 }`
    pub fn quality_limits(&self) -> EffectValueRange {
        self.quality_limits.update(DEFAULT_QUALITY_VALUE_RANGE)
    }

    /// 默认(schema): `{ low = -0.8, high = 1000 }`
    pub fn speed_limits(&self) -> EffectValueRange {
        self.speed_limits.update(DEFAULT_EFFECT_VALUE_RANGE)
    }
}

// ── SurfaceCondition ──────────────────────────────────────────────

impl SurfaceCondition {
    /// 默认(schema): Max double
    pub fn max(&self) -> f64 {
        self.max.unwrap_or(f64::MAX)
    }

    /// 默认(schema): Lowest double
    pub fn min(&self) -> f64 {
        self.min.unwrap_or(f64::MIN)
    }
}

// ── IconData ──────────────────────────────────────────────────────

impl IconData {
    /// 默认(schema): `{0, 0}`
    pub fn shift(&self) -> Vector {
        self.shift.unwrap_or(Vector(0.0, 0.0))
    }

    /// 默认(schema): `{r=1, g=1, b=1, a=1}`
    pub fn tint(&self) -> Color {
        self.tint.unwrap_or(WHITE)
    }
}

// ── PrototypeBaseComponent ────────────────────────────────────────

impl PrototypeBaseComponent {
    /// 默认(schema): Value of `hidden`
    pub fn hidden_in_factoriopedia(&self) -> bool {
        self.hidden_in_factoriopedia.unwrap_or(self.hidden)
    }
}

// ── EntityComponent ───────────────────────────────────────────────

impl EntityComponent {
    /// 默认(schema): nil (entity is not autoplacable)——None 语义真实，保留
    pub fn autoplace(&self) -> Option<&AutoplaceSpecification> {
        self.autoplace.as_ref()
    }

    /// 默认(schema): Empty = `{{0, 0}, {0, 0}}`
    pub fn collision_box(&self) -> BoundingBox {
        self.collision_box.unwrap_or(EMPTY_BOUNDING_BOX)
    }

    /// 默认(schema): not minable——None 语义真实，保留
    pub fn minable(&self) -> Option<&MinableProperties> {
        self.minable.as_ref()
    }

    /// 默认(schema): calculated by the collision box height rounded up.
    pub fn tile_height(&self) -> i32 {
        self.tile_height.unwrap_or_else(|| {
            let height = self
                .collision_box
                .as_ref()
                .map_or(0.0, |b| (b.1).1 - (b.0).1);
            height.ceil() as i32
        })
    }

    /// 默认(schema): calculated by the collision box width rounded up.
    pub fn tile_width(&self) -> i32 {
        self.tile_width.unwrap_or_else(|| {
            let width = self
                .collision_box
                .as_ref()
                .map_or(0.0, |b| (b.1).0 - (b.0).0);
            width.ceil() as i32
        })
    }
}

// ── 效果类型机器（AgriculturalTower/CraftingMachine/Beacon/Lab/MiningDrill）──

impl AgriculturalTowerComponent {
    /// 默认(schema): No effects are allowed
    pub fn allowed_effects(&self) -> EffectTypeLimitation {
        self.allowed_effects.unwrap_or(NO_EFFECTS)
    }

    /// 默认(schema): All module categories are allowed——None = 全部允许，语义真实，保留
    pub fn allowed_module_categories(&self) -> Option<&[String]> {
        self.allowed_module_categories.as_deref()
    }
}

impl CraftingMachineComponent {
    /// 默认(schema): No effects are allowed
    pub fn allowed_effects(&self) -> EffectTypeLimitation {
        self.allowed_effects.unwrap_or(NO_EFFECTS)
    }

    /// 默认(schema): All module categories are allowed——None = 全部允许，语义真实，保留
    pub fn allowed_module_categories(&self) -> Option<&[String]> {
        self.allowed_module_categories.as_deref()
    }
}

impl BeaconComponent {
    /// 默认(schema): No effects are allowed
    pub fn allowed_effects(&self) -> EffectTypeLimitation {
        self.allowed_effects.unwrap_or(NO_EFFECTS)
    }

    /// 默认(schema): All module categories are allowed——None = 全部允许，语义真实，保留
    pub fn allowed_module_categories(&self) -> Option<&[String]> {
        self.allowed_module_categories.as_deref()
    }

    pub fn get_profile(&self, beacon_count: usize) -> f64 {
        if self.profile.is_empty() {
            return 1.0;
        }
        if beacon_count < self.profile.len() {
            self.profile[beacon_count]
        } else {
            *self.profile.last().unwrap()
        }
    }
}

impl LabComponent {
    /// 默认(schema): All effects except quality are allowed
    pub fn allowed_effects(&self) -> EffectTypeLimitation {
        self.allowed_effects.unwrap_or(LAB_EFFECTS)
    }

    /// 默认(schema): All module categories are allowed——None = 全部允许，语义真实，保留
    pub fn allowed_module_categories(&self) -> Option<&[String]> {
        self.allowed_module_categories.as_deref()
    }
}

impl MiningDrillComponent {
    /// 默认(schema): All effects are allowed
    pub fn allowed_effects(&self) -> EffectTypeLimitation {
        self.allowed_effects.unwrap_or(MINING_EFFECTS)
    }

    /// 默认(schema): All module categories are allowed——None = 全部允许，语义真实，保留
    pub fn allowed_module_categories(&self) -> Option<&[String]> {
        self.allowed_module_categories.as_deref()
    }
}

// ── FluidComponent ────────────────────────────────────────────────
impl FluidComponent {
    /// 默认(schema): value of `default_temperature`
    pub fn max_temperature(&self) -> f64 {
        self.max_temperature.unwrap_or(self.default_temperature)
    }
}

// ── TransportBeltConnectable ──────────────────────────────────────

impl TransportBeltConnectableComponent {
    /// 默认(schema): Empty = `{{0, 0}, {0, 0}}`
    pub fn collision_box(&self) -> BoundingBox {
        self.collision_box.unwrap_or(EMPTY_BOUNDING_BOX)
    }
}

// ── QualityComponent ──────────────────────────────────────────────
// 公式见 schema 注释；`level` 默认 0，`next_probability`/`previous_probability`
// 默认 0（生成代码 default fn），`default_multiplier`/`inventory_size_multiplier`
// 未设置时沿依赖链回落（`default_multiplier` → `1 + 0.3 * level`）。

impl QualityComponent {
    /// 默认(schema): 1 + `level`
    pub fn accumulator_capacity_multiplier(&self) -> f64 {
        self.accumulator_capacity_multiplier
            .unwrap_or(1.0 + self.level as f64)
    }

    /// 默认(schema): Value of `level`
    pub fn asteroid_collector_collection_radius_bonus(&self) -> f64 {
        self.asteroid_collector_collection_radius_bonus
            .unwrap_or(self.level as f64)
    }

    /// 默认(schema): Value of `level`
    pub fn beacon_module_slots_bonus(&self) -> u16 {
        self.beacon_module_slots_bonus.unwrap_or(self.level as u16)
    }

    /// 默认(schema): clamp(`level`, 0, 64)
    pub fn beacon_supply_area_distance_bonus(&self) -> f64 {
        self.beacon_supply_area_distance_bonus
            .unwrap_or_else(|| (self.level as f64).clamp(0.0, 64.0))
    }

    /// 默认(schema): Value of `inventory_size_multiplier`
    pub fn cargo_wagon_inventory_size_multiplier(&self) -> f64 {
        self.cargo_wagon_inventory_size_multiplier
            .unwrap_or_else(|| self.inventory_size_multiplier())
    }

    /// 默认(schema): clamp(`next_probability * 0.1, 0, 1)`
    pub fn chain_probability(&self) -> f64 {
        self.chain_probability
            .unwrap_or_else(|| (self.next_probability * 0.1).clamp(0.0, 1.0))
    }

    /// 默认(schema): Value of `level`
    pub fn crafting_machine_module_slots_bonus(&self) -> u16 {
        self.crafting_machine_module_slots_bonus
            .unwrap_or(self.level as u16)
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn crafting_machine_speed_multiplier(&self) -> f64 {
        self.crafting_machine_speed_multiplier
            .unwrap_or_else(|| self.default_multiplier())
    }

    /// 默认(schema): 1 + 0.3 * `level`
    pub fn default_multiplier(&self) -> f64 {
        self.default_multiplier
            .unwrap_or(1.0 + 0.3 * self.level as f64)
    }

    /// 默认(schema): Value of `level`
    pub fn electric_pole_supply_area_distance_bonus(&self) -> f64 {
        self.electric_pole_supply_area_distance_bonus
            .unwrap_or(self.level as f64)
    }

    /// 默认(schema): 2 * `level`
    pub fn electric_pole_wire_reach_bonus(&self) -> f64 {
        self.electric_pole_wire_reach_bonus
            .unwrap_or(2.0 * self.level as f64)
    }

    /// 默认(schema): Value of `level`
    pub fn equipment_grid_height_bonus(&self) -> i16 {
        self.equipment_grid_height_bonus
            .unwrap_or(self.level as i16)
    }

    /// 默认(schema): Value of `level`
    pub fn equipment_grid_width_bonus(&self) -> i16 {
        self.equipment_grid_width_bonus.unwrap_or(self.level as i16)
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn fluid_wagon_capacity_multiplier(&self) -> f64 {
        self.fluid_wagon_capacity_multiplier
            .unwrap_or_else(|| self.default_multiplier())
    }

    /// 默认(schema): 1 + `level`
    pub fn flying_robot_max_energy_multiplier(&self) -> f64 {
        self.flying_robot_max_energy_multiplier
            .unwrap_or(1.0 + self.level as f64)
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn inserter_speed_multiplier(&self) -> f64 {
        self.inserter_speed_multiplier
            .unwrap_or_else(|| self.default_multiplier())
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn inventory_size_multiplier(&self) -> f64 {
        self.inventory_size_multiplier
            .unwrap_or_else(|| self.default_multiplier())
    }

    /// 默认(schema): Value of `level`
    pub fn lab_module_slots_bonus(&self) -> u16 {
        self.lab_module_slots_bonus.unwrap_or(self.level as u16)
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn lab_research_speed_multiplier(&self) -> f64 {
        self.lab_research_speed_multiplier
            .unwrap_or_else(|| self.default_multiplier())
    }

    /// 默认(schema): 1 + 0.01 * `level`
    pub fn locomotive_power_multiplier(&self) -> f64 {
        self.locomotive_power_multiplier
            .unwrap_or(1.0 + 0.01 * self.level as f64)
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn logistic_cell_charging_energy_multiplier(&self) -> f64 {
        self.logistic_cell_charging_energy_multiplier
            .unwrap_or_else(|| self.default_multiplier())
    }

    /// 默认(schema): Value of `level`
    pub fn logistic_cell_charging_station_count_bonus(&self) -> u32 {
        self.logistic_cell_charging_station_count_bonus
            .unwrap_or(self.level)
    }

    /// 默认(schema): Value of `level`
    pub fn mining_drill_mining_radius_bonus(&self) -> f64 {
        self.mining_drill_mining_radius_bonus
            .unwrap_or(self.level as f64)
    }

    /// 默认(schema): Value of `level`
    pub fn mining_drill_module_slots_bonus(&self) -> u16 {
        self.mining_drill_module_slots_bonus
            .unwrap_or(self.level as u16)
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn module_consumption_multiplier(&self) -> f64 {
        self.module_consumption_multiplier
            .unwrap_or_else(|| self.default_multiplier())
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn module_pollution_multiplier(&self) -> f64 {
        self.module_pollution_multiplier
            .unwrap_or_else(|| self.default_multiplier())
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn module_productivity_multiplier(&self) -> f64 {
        self.module_productivity_multiplier
            .unwrap_or_else(|| self.default_multiplier())
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn module_quality_multiplier(&self) -> f64 {
        self.module_quality_multiplier
            .unwrap_or_else(|| self.default_multiplier())
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn module_speed_multiplier(&self) -> f64 {
        self.module_speed_multiplier
            .unwrap_or_else(|| self.default_multiplier())
    }

    /// 默认(schema): clamp(`previous_probability * 0.1, 0, 1)`
    pub fn previous_chain_probability(&self) -> f64 {
        self.previous_chain_probability
            .unwrap_or((self.previous_probability * 0.1).clamp(0.0, 1.0))
    }

    /// 默认(schema): min(1 + 0.1 * `level`, 3)
    pub fn range_multiplier(&self) -> f64 {
        self.range_multiplier
            .unwrap_or((1.0 + 0.1 * self.level as f64).min(3.0))
    }

    /// 默认(schema): 1 + 0.01 * `level`
    pub fn rolling_stock_max_speed_multiplier(&self) -> f64 {
        self.rolling_stock_max_speed_multiplier
            .unwrap_or(1.0 + 0.01 * self.level as f64)
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn spoil_ticks_multiplier(&self) -> f64 {
        self.spoil_ticks_multiplier
            .unwrap_or_else(|| self.default_multiplier())
    }

    /// 默认(schema): 1 + `level`
    pub fn tool_durability_multiplier(&self) -> f64 {
        self.tool_durability_multiplier
            .unwrap_or(1.0 + self.level as f64)
    }
}

// ── Recipe ────────────────────────────────────────────────────────

impl RecipeComponent {
    /// 默认(schema): All module categories are allowed——None = 全部允许，语义真实，保留
    pub fn allowed_module_categories(&self) -> Option<&[String]> {
        self.allowed_module_categories.as_deref()
    }

    /// 默认(schema): `{"crafting"}`
    pub fn categories(&self) -> &[String] {
        self.categories
            .as_deref()
            .unwrap_or(&CRAFTING_CATEGORIES[..])
    }

    /// 默认(schema): Value of `hidden`
    pub fn requires_ingredients_to_unlock_results(&self) -> bool {
        self.requires_ingredients_to_unlock_results
            .unwrap_or(self.hidden)
    }
}

// ── RocketSilo ────────────────────────────────────────────────────

impl RocketSiloComponent {
    /// 默认(schema): Value of `rocket_parts_required`
    pub fn rocket_parts_storage_cap(&self) -> u32 {
        self.rocket_parts_storage_cap
            .unwrap_or(self.rocket_parts_required)
    }
}

impl ModuleComponent {
    pub fn consumption_quality_multiplier(&self) -> f64 {
        self.consumption_quality_multiplier
            .unwrap_or(if self.effect.consumption < 0.0 {1.0} else {0.0})
    }

    pub fn pollution_quality_multiplier(&self) -> f64 {
        self.pollution_quality_multiplier
            .unwrap_or(if self.effect.pollution < 0.0 {1.0} else {0.0})
    }

    pub fn productivity_quality_multiplier(&self) -> f64 {
        self.productivity_quality_multiplier
            .unwrap_or(if self.effect.productivity > 0.0 {1.0} else {0.0})
    }

    pub fn speed_quality_multiplier(&self) -> f64 {
        self.speed_quality_multiplier
            .unwrap_or(if self.effect.speed > 0.0 {1.0} else {0.0})
    }

    pub fn quality_quality_multiplier(&self) -> f64 {
        self.quality_quality_multiplier
            .unwrap_or(if self.effect.quality > 0.0 {1.0} else {0.0})
    }
}

// ── literal 默认值（schema literal 默认；初扫只扫 Text 注释，这些被漏掉）──

const ZERO_ENERGY: EnergyAmount = EnergyAmount { amount: 0.0 };
/// 1kJ = 1000 J（一单位流体上升一摄氏度所需的能量）。
const KILOJOUL: EnergyAmount = EnergyAmount { amount: 1000.0 };

impl EntityComponent {
    /// 默认(literal): "0W"——该实体冻结需要的加热能量，0 表示不可冻结。
    pub fn heating_energy(&self) -> EnergyAmount {
        self.heating_energy.unwrap_or(ZERO_ENERGY)
    }
}

impl FluidComponent {
    /// 默认(literal): "0J"——不作为燃料。
    pub fn fuel_value(&self) -> EnergyAmount {
        self.fuel_value.unwrap_or(ZERO_ENERGY)
    }

    /// 默认(literal): "1kJ"——每单位流体每摄氏度。
    pub fn heat_capacity(&self) -> EnergyAmount {
        self.heat_capacity.unwrap_or(KILOJOUL)
    }
}

impl ItemComponent {
    /// 默认(literal): "0J"——不作为燃料。
    pub fn fuel_value(&self) -> EnergyAmount {
        self.fuel_value.unwrap_or(ZERO_ENERGY)
    }
}

impl LoaderComponent {
    /// 默认(literal): 0——每件物品所需能量。
    pub fn energy_per_item(&self) -> EnergyAmount {
        self.energy_per_item.unwrap_or(ZERO_ENERGY)
    }
}

// ── 发电机：流体 → 电量（迁移自 metatorio-egui GeneratorPrototype::get_output）──

/// 发电机的每秒输出。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GeneratorOutput {
    /// 每秒消耗的流体量。
    pub fluid_used_per_second: f64,
    /// 每秒产生的电量（焦耳）。
    pub power_per_second: f64,
}

impl GeneratorComponent {
    /// 输入指定温度的流体时，流体的消耗量和电量产出（每秒）。
    ///
    /// `fluid_name` 用于与 `fluid_box.filter` 匹配（Component 不含 name，name 在 record 层）。
    pub fn get_output(
        &self,
        fluid_name: &str,
        fluid: &FluidComponent,
        temperature: f64,
    ) -> GeneratorOutput {
        let mut scale = 1.0;
        if self.burns_fluid {
            // 直接燃烧流体产生电力的发电机
            let fuel_value = fluid.fuel_value();
            let actual_power_output = EnergyAmount {
                amount: self.fluid_usage_per_tick * fuel_value.amount * self.effectivity,
            };
            if self.scale_fluid_usage
                && let Some(max_power_output) = self.max_power_output
            {
                if actual_power_output > max_power_output {
                    scale = max_power_output.amount / actual_power_output.amount;
                    return GeneratorOutput {
                        fluid_used_per_second: self.fluid_usage_per_tick * scale * 60.0,
                        power_per_second: max_power_output.amount * 60.0,
                    };
                }
                GeneratorOutput {
                    fluid_used_per_second: self.fluid_usage_per_tick * scale * 60.0,
                    power_per_second: actual_power_output.amount * 60.0,
                }
            } else {
                if let Some(max_power_output) = self.max_power_output
                    && actual_power_output > max_power_output
                {
                    return GeneratorOutput {
                        fluid_used_per_second: self.fluid_usage_per_tick * 60.0,
                        power_per_second: max_power_output.amount * 60.0,
                    };
                }
                GeneratorOutput {
                    fluid_used_per_second: self.fluid_usage_per_tick * 60.0,
                    power_per_second: actual_power_output.amount * 60.0,
                }
            }
        } else {
            // 靠热量差产生电力的发电机
            let heat_capacity = fluid.heat_capacity();
            let max_power_output = if let Some(max_power_output) = self.max_power_output {
                max_power_output
            } else {
                let filter = self.fluid_box.filter.as_ref().unwrap();
                if fluid_name != filter {
                    // 如果流体不符合过滤条件，则不产生电力
                    if self.destroy_non_fuel_fluid {
                        return GeneratorOutput {
                            fluid_used_per_second: self.fluid_usage_per_tick * 60.0,
                            power_per_second: 0.0,
                        };
                    } else {
                        return GeneratorOutput {
                            fluid_used_per_second: 0.0,
                            power_per_second: 0.0,
                        };
                    }
                }
                let max_temperature = if let Some(max_temperature) = fluid.max_temperature {
                    max_temperature.min(self.maximum_temperature)
                } else {
                    self.maximum_temperature
                };
                let temperature_diff = max_temperature - fluid.default_temperature;
                EnergyAmount {
                    amount: temperature_diff
                        * self.fluid_usage_per_tick
                        * heat_capacity.amount
                        * self.effectivity,
                }
            };
            let actual_power_output = EnergyAmount {
                amount: (temperature - fluid.default_temperature)
                    * self.fluid_usage_per_tick
                    * heat_capacity.amount
                    * self.effectivity,
            };

            if self.scale_fluid_usage {
                if actual_power_output > max_power_output {
                    scale = max_power_output.amount / actual_power_output.amount;
                    return GeneratorOutput {
                        fluid_used_per_second: self.fluid_usage_per_tick * scale * 60.0,
                        power_per_second: max_power_output.amount * 60.0,
                    };
                }
                GeneratorOutput {
                    fluid_used_per_second: self.fluid_usage_per_tick * scale * 60.0,
                    power_per_second: actual_power_output.amount * 60.0,
                }
            } else {
                if actual_power_output > max_power_output {
                    return GeneratorOutput {
                        fluid_used_per_second: self.fluid_usage_per_tick * 60.0,
                        power_per_second: max_power_output.amount * 60.0,
                    };
                }
                GeneratorOutput {
                    fluid_used_per_second: self.fluid_usage_per_tick * 60.0,
                    power_per_second: actual_power_output.amount * 60.0,
                }
            }
        }
    }
}

// ── 锅炉：加热输出量（不与流类型绑定，调用方手动转换为流）──

/// 锅炉加热输出（`output-to-separate-pipe` 模式）。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FluidHeatingOutput {
    /// 输入流体每秒消耗量（转换为流时为负向）。
    pub input_amount_per_second: f64,
    /// 输出流体每秒产出量（转换为流时为正向）。
    pub output_amount_per_second: f64,
    /// 输出流体温度。
    pub output_temperature: f64,
}

impl BoilerComponent {
    /// `output-to-separate-pipe` 模式：每秒加热的流体量。
    ///
    /// 输入/输出流体名与 `output_fluid_box.filter` 逻辑由调用方决定（数据层只做计算）。
    /// 迁移自 metatorio-egui BoilerPrototype::get_flow 的核心公式。
    /// 实际使用时，需要给流体温度添加对应流体类型的虚拟温度流
    /// heat-fluid-inside 模式则是始终产生同种流体的对应温度流，不需要特殊处理
    pub fn heating_output(
        &self,
        input_fluid: &FluidComponent,
        output_fluid: &FluidComponent,
        input_temperature: f64,
    ) -> Option<FluidHeatingOutput> {
        let target_temperature = self.target_temperature?;
        if target_temperature - input_temperature == 0.0 {
            return None;
        }
        let source_capacity = input_fluid.heat_capacity().amount;
        let target_capacity = output_fluid.heat_capacity().amount;
        let amount = self.energy_consumption.amount * 60.0 // 功率
            / source_capacity // 输入流体的比热容
            / (target_temperature - input_temperature); // 温度差
        Some(FluidHeatingOutput {
            input_amount_per_second: amount,
            output_amount_per_second: amount * source_capacity / target_capacity,
            output_temperature: target_temperature,
        })
    }
}