//! Phase 3 集成测试：PrototypeStore 加载 fixture dump，
//! 验证 (group, name) 主键、聚合标签与跨键分组。

use metatorio_data::generated_components::{
    ComponentValue, CraftingMachineComponent, ItemComponent, PrototypeBaseComponent,
};
use metatorio_data::store::{PrototypeGroup, PrototypeStore};
use serde_json::Value;

fn load_fixture() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/prototypes-fixture.json"
    );
    let text = std::fs::read_to_string(path).expect("fixture 文件不存在");
    serde_json::from_str(&text).expect("fixture 解析失败")
}

#[test]
fn assembling_machine_is_two_records() {
    let store = PrototypeStore::load(&load_fixture()).expect("fixture 加载失败");

    // Entity 组：assembling-machine 键 → 含制造机器组件
    let entity = store
        .entity("assembling-machine-1")
        .expect("应有 entity 组记录");
    assert_eq!(entity.type_, "assembling-machine");
    assert!(
        entity.has("CraftingMachineComponent"),
        "entity 记录应含 Crafter 组件"
    );
    let crafter = entity
        .component::<CraftingMachineComponent>()
        .expect("Crafter 组件");
    assert!(
        (crafter.crafting_speed - 0.5).abs() < 1e-9,
        "组装机1 速度 0.5"
    );
    match entity.get("CraftingMachineComponent") {
        Some(ComponentValue::CraftingMachineComponent(_)) => {}
        _ => panic!("ComponentValue 变体不匹配"),
    }

    // Item 组：item 键 → 含物品组件（同名不同组 = 不同原型）
    let item = store
        .item("assembling-machine-1")
        .expect("应有 item 组记录");
    assert_eq!(item.type_, "item");
    assert!(item.has("ItemComponent"));
    assert!(
        item.component::<ItemComponent>().is_some(),
        "item 记录应含 Item 组件"
    );

    // 两条记录必须不同（主键 (group, name)）
    assert_ne!(entity.group, item.group);
    dbg!(item);
}

#[test]
fn speed_module_is_three_records() {
    let store = PrototypeStore::load(&load_fixture()).expect("fixture 加载失败");

    // "speed-module" 是三个不同原型：recipe / module / technology
    // （module 是 ItemPrototype 子类 → 聚合到 Item 组，LuaPrototypes 语义）
    let recipe = store
        .get(PrototypeGroup::Recipe, "speed-module")
        .expect("recipe 组记录");
    let module = store
        .item("speed-module")
        .expect("module 键原型聚合到 Item 组");
    let technology = store
        .get(PrototypeGroup::Technology, "speed-module")
        .expect("technology 组记录");
    assert!(recipe.has("RecipeComponent"));
    assert!(
        module.has("ModuleComponent"),
        "module 记录应含 ModuleComponent"
    );
    assert!(technology.has("TechnologyComponent"));

    // 组件互不相同（证明不是同一原型）
    assert!(!recipe.has("ModuleComponent"));
    assert!(!module.has("RecipeComponent"));
}

#[test]
fn group_iteration_and_length() {
    let store = PrototypeStore::load(&load_fixture()).expect("fixture 加载失败");
    assert!(!store.is_empty());

    let entity_count = store.group(PrototypeGroup::Entity).count();
    let item_count = store.group(PrototypeGroup::Item).count();
    let other_count = store.group(PrototypeGroup::Recipe).count();
    assert!(entity_count > 0, "应有实体记录");
    assert!(item_count > 0, "应有物品记录");
    assert!(other_count > 0, "应有配方记录");

    // 总数 = 各组之和（Entity + Item + 全部 Other）
    assert!(store.len() >= entity_count + item_count);

    dbg!(
        store
            .groups.keys().cloned().collect::<std::collections::HashSet<_>>()
    );
}

#[test]
fn load_error_reports_failures() {
    // 非法 dump（原型 JSON 不是对象）→ 加载报错且含失败明细
    let bad: Value = serde_json::json!({
        "item": {
            "broken-item": "not an object",
        }
    });
    let err = PrototypeStore::load(&bad).expect_err("非法 dump 应加载失败");
    assert!(!err.failures.is_empty());
    assert!(
        err.failures
            .iter()
            .any(|(t, n, _)| t == "item" && n == "broken-item")
    );
    assert_eq!(err.total, 1);
    assert_eq!(err.succeeded, 0);
}

#[test]
fn order_info_three_levels() {
    let store = PrototypeStore::load(&load_fixture()).expect("fixture 加载失败");
    let order_info = store.order_info();
    let reverse = store.reverse_order_info();
    // 三层结构：大组 → 小组 → 条目名
    let item_order = order_info
        .get(&PrototypeGroup::Item)
        .expect("Item 组应有排序信息");
    assert!(!item_order.is_empty(), "Item 组排序信息不应为空");
    for (group_name, subgroups) in item_order {
        assert!(!group_name.is_empty(), "大组名不应为空");
        for (subgroup_name, items) in subgroups {
            assert!(!items.is_empty(), "小组 {subgroup_name} 不应为空");
        }
    }

    // 反查索引：所有条目名都能查到三层索引，且索引与正查一致
    for (group, order) in order_info {
        let rev = &reverse[group];
        for (gi, (_, subgroups)) in order.iter().enumerate() {
            for (si, (_, items)) in subgroups.iter().enumerate() {
                for (ii, name) in items.iter().enumerate() {
                    assert_eq!(
                        rev.get(name),
                        Some(&(gi, si, ii)),
                        "反查索引与正查不一致：{name}"
                    );
                }
            }
        }
    }

    // 组内条目按 (order, name) 排序：取 Item 组第一个小组验证
    let first_subgroup = item_order.values().next().expect("有小组");
    let names = first_subgroup.values().next().expect("有条目");
    let orders: Vec<String> = names
        .iter()
        .map(|n| {
            store
                .item(n)
                .and_then(|r| r.component::<PrototypeBaseComponent>())
                .map(|b| b.order.clone())
                .unwrap_or_default()
        })
        .collect();
    assert!(
        orders.array_windows::<2>().all(|w| w[0] <= w[1]),
        "小组内条目应按 order 非降序排序"
    );
}

#[test]
fn technology_dependents_derivation() {
    let store = PrototypeStore::load(&load_fixture()).expect("fixture 加载失败");
    let dependents = store.technology_dependents();

    // 科技只声明 prerequisites；反向依赖应被预处理出来
    assert_eq!(
        dependents.get("automation-science-pack").map(|v| v.as_slice()),
        Some(&["automation".to_string(), "logistics".to_string()][..])
    );
    assert_eq!(
        dependents.get("modules").map(|v| v.as_slice()),
        Some(&["speed-module".to_string()][..])
    );
    // 惰性：再次调用返回同一实例
    assert!(std::ptr::eq(
        store.technology_dependents(),
        store.technology_dependents()
    ));
}
