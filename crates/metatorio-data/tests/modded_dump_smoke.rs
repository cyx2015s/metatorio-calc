//! 重度 mod 数据冒烟测试：用带 mod 的真实 dump（`assets/data-raw-dump-heavily-modded.json`）
//! 验证 `PrototypeStore` 在"污染数据"下能完整加载。
//!
//! 该 dump 包含大量 mod 原型（物品/配方/机器数量远超原版），是 lenient 反序列化
//! 与自定义类型的真实样本（float→int、空 map、0-255 与 0-1 混用的颜色等）。
//! 加载失败的条目会汇总在 `LoadError` 中并断言为空——新增 mod 污染形态时在此暴露。

use metatorio_data::{
    CraftingMachineComponent,
    store::{PrototypeGroup, PrototypeStore},
};
use serde_json::Value;

fn load_dump() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/data-raw-dump-heavily-modded.json"
    );
    let text = std::fs::read_to_string(path)
        .expect("modded dump 不存在（assets/data-raw-dump-heavily-modded.json）");
    serde_json::from_str(&text).expect("modded dump 解析失败")
}

/// mod 用 `0` 关掉一个可选表字段——真实样本：casting_ladle 0.2.1 的
/// `data.raw["assembling-machine"]["foundry"].effect_receiver = 0`（注释：关闭熔炉自带产能）。
///
/// 游戏照常加载并把 `0` 当作「该字段没设置」；我们的加载器不允许因为一个字段的
/// 这种写法就让全部原型加载失败，所以按游戏语义归一（`lenient::de_opt_struct_lenient`）。
#[test]
fn zero_for_an_optional_table_is_read_as_unset() {
    let dump: Value = serde_json::from_str(
        r#"{
          "assembling-machine": {
            "foundry": { "crafting_speed": 2.5, "effect_receiver": 0 }
          }
        }"#,
    )
    .expect("内联 dump 解析失败");

    let store = PrototypeStore::load(&dump).expect("`effect_receiver = 0` 不应导致加载失败");
    let foundry = store.entity("foundry").expect("foundry 实体记录");
    let crafter = foundry
        .component::<CraftingMachineComponent>()
        .expect("foundry 应有制造机器组件");
    assert!(
        crafter.effect_receiver.is_none(),
        "`0` 应读成「未设置」，而不是报 invalid type"
    );
    assert!(
        (crafter.crafting_speed - 2.5).abs() < 1e-9,
        "同一个组件的其它字段不受影响：{}",
        crafter.crafting_speed
    );
}

#[test]
fn heavily_modded_dump_loads_all_concerned_prototypes() {
    let dump = load_dump();
    let store = PrototypeStore::load(&dump).expect("modded dump 加载失败（详见 LoadError）");

    eprintln!(
        "modded dump 冒烟：共 {} 条原型记录（Entity {} / Item {} / Other {}）",
        store.len(),
        store.group(PrototypeGroup::Entity).count(),
        store.group(PrototypeGroup::Item).count(),
        store.len()
            - store.group(PrototypeGroup::Entity).count()
            - store.group(PrototypeGroup::Item).count(),
    );

    assert!(!store.is_empty(), "modded dump 不应为空");
    assert!(
        store.entity("assembling-machine-1").is_some(),
        "原版实体应存在"
    );
    assert!(
        store.entity("se-delivery-cannon").is_some()
            || store.entity("kr-advanced-splitter").is_some(),
        "mod 实体应存在（SE/K2 污染样本）"
    );

    assert!(
        store
            .entity("stone-furnace")
            .unwrap()
            .component::<CraftingMachineComponent>()
            .is_some(),
        "原版炉子应有 Crafter 组件"
    );

    dbg!(
        store
            .groups
            .keys()
            .cloned()
            .collect::<std::collections::HashSet<_>>()
    );
}
