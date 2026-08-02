//! ext.rs 生效值（effective value）方法测试：
//! 字段已设置 → 返回字段值；未设置 → 回落 schema 注释默认值。

use metatorio_data::types::{BoundingBox, EffectType, Vector};
use metatorio_data::*;

#[test]
fn recipe_categories_effective() {
    let r = RecipeComponent::default();
    assert_eq!(r.categories(), &["crafting".to_string()][..]);

    let mut r = RecipeComponent::default();
    r.categories = Some(vec!["smelting".to_string()]);
    assert_eq!(r.categories(), &["smelting".to_string()][..]);
}

#[test]
fn quality_formulas() {
    // level 默认 0
    let q = QualityComponent::default();
    assert_eq!(q.default_multiplier(), 1.0);
    assert_eq!(q.accumulator_capacity_multiplier(), 1.0);
    assert_eq!(q.inventory_size_multiplier(), 1.0);
    assert_eq!(q.chain_probability(), 0.0);

    let mut q = QualityComponent::default();
    q.level = 3;
    assert_eq!(q.default_multiplier(), 1.9);
    assert_eq!(q.accumulator_capacity_multiplier(), 4.0);
    assert_eq!(q.electric_pole_wire_reach_bonus(), 6.0);
    assert_eq!(q.beacon_supply_area_distance_bonus(), 3.0);
    assert_eq!(q.range_multiplier(), 1.3);
    assert_eq!(q.tool_durability_multiplier(), 4.0);
    assert_eq!(q.locomotive_power_multiplier(), 1.03);

    // 字段有值时优先字段值
    let mut q = QualityComponent::default();
    q.level = 2;
    q.inventory_size_multiplier = Some(2.5);
    assert_eq!(q.inventory_size_multiplier(), 2.5);
    assert_eq!(q.cargo_wagon_inventory_size_multiplier(), 2.5);

    // 依赖链：cargo_wagon → inventory_size_multiplier → default_multiplier(1 + 0.3 * level)
    let mut q = QualityComponent::default();
    q.level = 2;
    assert_eq!(q.cargo_wagon_inventory_size_multiplier(), 1.6);
}

#[test]
fn entity_tile_size_from_collision_box() {
    let e = EntityComponent::default();
    assert_eq!(e.tile_width(), 0);
    assert_eq!(e.tile_height(), 0);

    let mut e = EntityComponent::default();
    e.collision_box = Some(BoundingBox(Vector(-1.0, -2.0), Vector(3.0, 4.0)));
    assert_eq!(e.tile_width(), 4);
    assert_eq!(e.tile_height(), 6);

    // 默认回落为空包围盒
    let e2 = EntityComponent::default();
    assert_eq!(
        e2.collision_box(),
        BoundingBox(Vector(0.0, 0.0), Vector(0.0, 0.0))
    );
    // autoplace / minable 默认 nil（不可自动放置/不可挖掘）
    assert!(e2.autoplace().is_none());
    assert!(e2.minable().is_none());
}

#[test]
fn base_hidden_default() {
    let b = PrototypeBaseComponent::default();
    assert_eq!(b.hidden_in_factoriopedia(), false);

    let mut b = PrototypeBaseComponent::default();
    b.hidden_in_factoriopedia = Some(true);
    assert_eq!(b.hidden_in_factoriopedia(), true);
}


#[test]
fn effect_receiver_limits() {
    let r = EffectReceiver::default();
    let v = r.quality_limits();
    assert_eq!(v.low, 0.0);
    assert_eq!(v.high, 1000.0);

    let v = r.speed_limits();
    assert_eq!(v.low, -0.8);
    assert_eq!(v.high, 1000.0);
}

#[test]
fn allowed_effects_defaults() {
    let m = MiningDrillComponent::default();
    let e = m.allowed_effects();
    assert!(e[EffectType::Quality]);

    let lab = LabComponent::default();
    let e = lab.allowed_effects();

    assert!(!e[EffectType::Quality]);

    let machine = CraftingMachineComponent::default();
    assert!(*machine.allowed_effects() == [false; 5]);
}
