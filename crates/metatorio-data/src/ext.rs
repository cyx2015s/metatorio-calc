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

use crate::types::{BoundingBox, Color, EffectTypeLimitation, EffectValueRangeOpt, Vector};

pub use crate::generated_components::*;

/// `{r=1, g=1, b=1, a=1}` 白色
const WHITE: Color = Color(255, 255, 255, 255);

/// 空碰撞盒（`{{0, 0}, {0, 0}}`，无碰撞）
const EMPTY_BOUNDING_BOX: BoundingBox = BoundingBox(Vector(0.0, 0.0), Vector(0.0, 0.0));

/// 默认(schema): No effects are allowed（含 Vec 有 Drop，const 不可提升为 `&'static`，用 static）
const NO_EFFECTS: EffectTypeLimitation = EffectTypeLimitation { allowed: [false; 5] };

/// 默认(schema): All effects except quality are allowed
const LAB_EFFECTS: EffectTypeLimitation = EffectTypeLimitation {
    allowed: [true, true, true, true, false],
};

/// 默认(schema): All effects are allowed
const MINING_EFFECTS: EffectTypeLimitation = EffectTypeLimitation {
    allowed: [true; 5],
};

/// 默认(schema): `{"crafting"}`（String 元素不可 const，惰性初始化）
static CRAFTING_CATEGORIES: LazyLock<Vec<String>> =
    LazyLock::new(|| vec!["crafting".to_string()]);

/// 默认(schema): {{1, 1, 1}}
const ARM_COLOR_GRADIENT: [Color; 1] = [WHITE];

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
pub trait EffectValueRangeExt {
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
    pub fn max(&self) -> Option<f64> {
        self.max.or(Some(f64::MAX))
    }

    /// 默认(schema): Lowest double
    pub fn min(&self) -> Option<f64> {
        self.min.or(Some(f64::MIN))
    }
}

// ── IconData ──────────────────────────────────────────────────────

impl IconData {
    // NOT_NEEDED(defaults): 默认(schema): `true` for the first layer, `false` otherwise
    // 依赖图标层序号（IconComponent.icons 中的位置），组件外数据。
    // pub fn draw_background(&self) -> Option<bool> { ... }

    /// 默认(schema): `{0, 0}`
    pub fn shift(&self) -> Option<Vector> {
        self.shift.or(Some(Vector(0.0, 0.0)))
    }

    /// 默认(schema): `{r=1, g=1, b=1, a=1}`
    pub fn tint(&self) -> Option<Color> {
        self.tint.or(Some(WHITE))
    }
}

// ── FluidBox ──────────────────────────────────────────────────────

impl FluidBox {
    // NOT_NEEDED(defaults): 默认(schema): Value of `UtilityConstants::default_pipeline_extent`
    // 游戏内部常量（组件外数据）。
    // pub fn max_pipeline_extent(&self) -> Option<u32> { ... }
}

// ── PrototypeBaseComponent ────────────────────────────────────────

impl PrototypeBaseComponent {
    /// 默认(schema): Value of `hidden`
    pub fn hidden_in_factoriopedia(&self) -> Option<bool> {
        self.hidden_in_factoriopedia.or(Some(self.hidden))
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
    pub fn tile_height(&self) -> Option<i32> {
        self.tile_height.or_else(|| {
            let height = self
                .collision_box
                .as_ref()
                .map_or(0.0, |b| (b.1).1 - (b.0).1);
            Some(height.ceil() as i32)
        })
    }

    /// 默认(schema): calculated by the collision box width rounded up.
    pub fn tile_width(&self) -> Option<i32> {
        self.tile_width.or_else(|| {
            let width = self
                .collision_box
                .as_ref()
                .map_or(0.0, |b| (b.1).0 - (b.0).0);
            Some(width.ceil() as i32)
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

// ── AsteroidChunk / AsteroidCollector ─────────────────────────────

impl AsteroidChunkComponent {
    /// 默认(schema): unset
    pub fn hide_from_signal_gui(&self) -> Option<bool> {
        self.hide_from_signal_gui
    }
}

impl AsteroidCollectorComponent {
    /// 默认(schema): {{1, 1, 1}}
    pub fn arm_color_gradient(&self) -> &[Color] {
        self.arm_color_gradient.as_deref().unwrap_or(&ARM_COLOR_GRADIENT)
    }
}

// ── FluidComponent ────────────────────────────────────────────────

impl FluidComponent {
    /// 默认(schema): max value of float
    pub fn gas_temperature(&self) -> Option<f64> {
        self.gas_temperature.or(Some(f32::MAX as f64))
    }

    /// 默认(schema): value of `default_temperature`
    pub fn max_temperature(&self) -> Option<f64> {
        self.max_temperature.or(Some(self.default_temperature))
    }
}

// ── ItemGroup / Item / ItemWithLabel ──────────────────────────────

impl ItemGroupComponent {
    // NOT_NEEDED(defaults): 默认(schema): The `order` of this item group.
    // 依赖 PrototypeBaseComponent.order（本组件无 order 字段）。
    // pub fn order_in_recipe(&self) -> Option<String> { ... }
}

impl ItemComponent {
    // NOT_NEEDED(defaults): 默认(schema): Value of UtilityConstants::item_default_random_tint_strength
    // 游戏内部常量（组件外数据）。
    // pub fn random_tint_color(&self) -> Option<Color> { ... }
}

impl ItemWithLabelComponent {
    // NOT_NEEDED(defaults): 默认(schema): Default item text color
    // 游戏内部常量（组件外数据）。
    // pub fn default_label_color(&self) -> Option<Color> { ... }
}

// ── TransportBeltConnectable ──────────────────────────────────────

impl TransportBeltConnectableComponent {
    /// 默认(schema): Empty = `{{0, 0}, {0, 0}}`
    pub fn collision_box(&self) -> BoundingBox {
        self.collision_box.unwrap_or(EMPTY_BOUNDING_BOX)
    }
}

// ── SpaceLocation / Planet ────────────────────────────────────────

impl SpaceLocationComponent {
    /// 默认(schema): same as orientation
    pub fn parked_platforms_orientation(&self) -> Option<f64> {
        self.parked_platforms_orientation.or(Some(self.orientation))
    }
}

impl PlanetComponent {
    // NOT_NEEDED(defaults): 默认(schema): CRC checksum of `name`
    // 依赖 PrototypeBaseComponent.name（本组件无 name 字段）。
    // pub fn map_seed_offset(&self) -> Option<u32> { ... }
}

// ── Pump ──────────────────────────────────────────────────────────

impl PumpComponent {
    /// 默认(schema): 1 / 64.0
    pub fn fluid_wagon_connector_speed(&self) -> Option<f64> {
        self.fluid_wagon_connector_speed.or(Some(1.0 / 64.0))
    }
}

// ── QualityComponent ──────────────────────────────────────────────
// 公式见 schema 注释；`level` 默认 0，`next_probability`/`previous_probability`
// 默认 0（生成代码 default fn），`default_multiplier`/`inventory_size_multiplier`
// 未设置时沿依赖链回落（`default_multiplier` → `1 + 0.3 * level`）。

impl QualityComponent {
    /// 默认(schema): 1 + `level`
    pub fn accumulator_capacity_multiplier(&self) -> Option<f64> {
        self.accumulator_capacity_multiplier.or(Some(1.0 + self.level as f64))
    }

    /// 默认(schema): Value of `level`
    pub fn asteroid_collector_collection_radius_bonus(&self) -> Option<f64> {
        self.asteroid_collector_collection_radius_bonus
            .or(Some(self.level as f64))
    }

    /// 默认(schema): Value of `level`
    pub fn beacon_module_slots_bonus(&self) -> Option<u16> {
        self.beacon_module_slots_bonus.or(Some(self.level as u16))
    }

    /// 默认(schema): clamp(`level`, 0, 64)
    pub fn beacon_supply_area_distance_bonus(&self) -> Option<f64> {
        self.beacon_supply_area_distance_bonus
            .or(Some((self.level as f64).clamp(0.0, 64.0)))
    }

    /// 默认(schema): Value of `inventory_size_multiplier`
    pub fn cargo_wagon_inventory_size_multiplier(&self) -> Option<f64> {
        self.cargo_wagon_inventory_size_multiplier
            .or_else(|| self.inventory_size_multiplier())
    }

    /// 默认(schema): clamp(`next_probability * 0.1, 0, 1)`
    pub fn chain_probability(&self) -> Option<f64> {
        self.chain_probability
            .or(Some((self.next_probability * 0.1).clamp(0.0, 1.0)))
    }

    /// 默认(schema): Value of `level`
    pub fn crafting_machine_module_slots_bonus(&self) -> Option<u16> {
        self.crafting_machine_module_slots_bonus
            .or(Some(self.level as u16))
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn crafting_machine_speed_multiplier(&self) -> Option<f64> {
        self.crafting_machine_speed_multiplier
            .or_else(|| self.default_multiplier())
    }

    /// 默认(schema): 1 + 0.3 * `level`
    pub fn default_multiplier(&self) -> Option<f64> {
        self.default_multiplier.or(Some(1.0 + 0.3 * self.level as f64))
    }

    /// 默认(schema): Value of `level`
    pub fn electric_pole_supply_area_distance_bonus(&self) -> Option<f64> {
        self.electric_pole_supply_area_distance_bonus
            .or(Some(self.level as f64))
    }

    /// 默认(schema): 2 * `level`
    pub fn electric_pole_wire_reach_bonus(&self) -> Option<f64> {
        self.electric_pole_wire_reach_bonus
            .or(Some(2.0 * self.level as f64))
    }

    /// 默认(schema): Value of `level`
    pub fn equipment_grid_height_bonus(&self) -> Option<i16> {
        self.equipment_grid_height_bonus.or(Some(self.level as i16))
    }

    /// 默认(schema): Value of `level`
    pub fn equipment_grid_width_bonus(&self) -> Option<i16> {
        self.equipment_grid_width_bonus.or(Some(self.level as i16))
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn fluid_wagon_capacity_multiplier(&self) -> Option<f64> {
        self.fluid_wagon_capacity_multiplier
            .or_else(|| self.default_multiplier())
    }

    /// 默认(schema): 1 + `level`
    pub fn flying_robot_max_energy_multiplier(&self) -> Option<f64> {
        self.flying_robot_max_energy_multiplier
            .or(Some(1.0 + self.level as f64))
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn inserter_speed_multiplier(&self) -> Option<f64> {
        self.inserter_speed_multiplier
            .or_else(|| self.default_multiplier())
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn inventory_size_multiplier(&self) -> Option<f64> {
        self.inventory_size_multiplier
            .or_else(|| self.default_multiplier())
    }

    /// 默认(schema): Value of `level`
    pub fn lab_module_slots_bonus(&self) -> Option<u16> {
        self.lab_module_slots_bonus.or(Some(self.level as u16))
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn lab_research_speed_multiplier(&self) -> Option<f64> {
        self.lab_research_speed_multiplier
            .or_else(|| self.default_multiplier())
    }

    /// 默认(schema): 1 + 0.01 * `level`
    pub fn locomotive_power_multiplier(&self) -> Option<f64> {
        self.locomotive_power_multiplier
            .or(Some(1.0 + 0.01 * self.level as f64))
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn logistic_cell_charging_energy_multiplier(&self) -> Option<f64> {
        self.logistic_cell_charging_energy_multiplier
            .or_else(|| self.default_multiplier())
    }

    /// 默认(schema): Value of `level`
    pub fn logistic_cell_charging_station_count_bonus(&self) -> Option<u32> {
        self.logistic_cell_charging_station_count_bonus
            .or(Some(self.level))
    }

    /// 默认(schema): Value of `level`
    pub fn mining_drill_mining_radius_bonus(&self) -> Option<f64> {
        self.mining_drill_mining_radius_bonus
            .or(Some(self.level as f64))
    }

    /// 默认(schema): Value of `level`
    pub fn mining_drill_module_slots_bonus(&self) -> Option<u16> {
        self.mining_drill_module_slots_bonus.or(Some(self.level as u16))
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn module_consumption_multiplier(&self) -> Option<f64> {
        self.module_consumption_multiplier
            .or_else(|| self.default_multiplier())
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn module_pollution_multiplier(&self) -> Option<f64> {
        self.module_pollution_multiplier
            .or_else(|| self.default_multiplier())
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn module_productivity_multiplier(&self) -> Option<f64> {
        self.module_productivity_multiplier
            .or_else(|| self.default_multiplier())
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn module_quality_multiplier(&self) -> Option<f64> {
        self.module_quality_multiplier
            .or_else(|| self.default_multiplier())
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn module_speed_multiplier(&self) -> Option<f64> {
        self.module_speed_multiplier
            .or_else(|| self.default_multiplier())
    }

    /// 默认(schema): clamp(`previous_probability * 0.1, 0, 1)`
    pub fn previous_chain_probability(&self) -> Option<f64> {
        self.previous_chain_probability
            .or(Some((self.previous_probability * 0.1).clamp(0.0, 1.0)))
    }

    /// 默认(schema): min(1 + 0.1 * `level`, 3)
    pub fn range_multiplier(&self) -> Option<f64> {
        self.range_multiplier
            .or(Some((1.0 + 0.1 * self.level as f64).min(3.0)))
    }

    /// 默认(schema): 1 + 0.01 * `level`
    pub fn rolling_stock_max_speed_multiplier(&self) -> Option<f64> {
        self.rolling_stock_max_speed_multiplier
            .or(Some(1.0 + 0.01 * self.level as f64))
    }

    /// 默认(schema): Value of `default_multiplier`
    pub fn spoil_ticks_multiplier(&self) -> Option<f64> {
        self.spoil_ticks_multiplier.or_else(|| self.default_multiplier())
    }

    /// 默认(schema): 1 + `level`
    pub fn tool_durability_multiplier(&self) -> Option<f64> {
        self.tool_durability_multiplier.or(Some(1.0 + self.level as f64))
    }
}

// ── Reactor ───────────────────────────────────────────────────────

impl ReactorComponent {
    /// 默认(schema): `{1, 1, 1, 1} (white)`
    pub fn default_fuel_glow_color(&self) -> Option<Color> {
        self.default_fuel_glow_color.or(Some(WHITE))
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
        self.categories.as_deref().unwrap_or(&CRAFTING_CATEGORIES[..])
    }

    /// 默认(schema): Value of `hidden`
    pub fn requires_ingredients_to_unlock_results(&self) -> Option<bool> {
        self.requires_ingredients_to_unlock_results.or(Some(self.hidden))
    }
}

// ── RocketSilo ────────────────────────────────────────────────────

impl RocketSiloComponent {
    /// 默认(schema): Value of `rocket_parts_required`
    pub fn rocket_parts_storage_cap(&self) -> Option<u32> {
        self.rocket_parts_storage_cap.or(Some(self.rocket_parts_required))
    }
}

// ── SurfaceProperty ───────────────────────────────────────────────

impl SurfacePropertyComponent {
    // NOT_NEEDED(defaults): 默认(schema): surface-property-unit.[prototype name]
    // 依赖 PrototypeBaseComponent.name（本组件无 name 字段）。
    // pub fn localised_unit_key(&self) -> Option<String> { ... }
}

// ── Tile ──────────────────────────────────────────────────────────

impl TileComponent {
    /// 默认(schema): `{r=1, g=1, b=1, a=1} (white)`
    pub fn effect_color(&self) -> Option<Color> {
        self.effect_color.or(Some(WHITE))
    }

    /// 默认(schema): true if `effect_color` alpha equals 1
    pub fn effect_is_opaque(&self) -> Option<bool> {
        self.effect_is_opaque.or_else(|| {
            Some(self.effect_color.map_or(true, |c| c.3 == 255))
        })
    }

    /// 默认(schema): `{r=1, g=1, b=1, a=1} (white)`
    pub fn tint(&self) -> Option<Color> {
        self.tint.or(Some(WHITE))
    }
}
