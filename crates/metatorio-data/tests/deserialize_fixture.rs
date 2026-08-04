//! fixture 反序列化测试：用真实游戏 dump 的裁剪数据验证
//! 生成的组件结构体能正确反序列化（忠实层的正确性锚点）。
//!
//! fixture 由 make_fixture.py 从完整 data-raw-dump.json 裁剪生成，
//! 提交进仓库，CI 可离线运行。

use metatorio_data::generated_components::*;
use serde_json::Value;

fn load_fixture() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/prototypes-fixture.json"
    );
    let text = std::fs::read_to_string(path).expect("fixture 文件不存在");
    serde_json::from_str(&text).expect("fixture 解析失败")
}

/// 从 dump 顶层键取原型 JSON。
fn proto<'a>(data: &'a Value, key: &str, name: &str) -> &'a Value {
    data.get(key)
        .and_then(|k| k.get(name))
        .unwrap_or_else(|| panic!("fixture 缺少 {key}/{name}"))
}

#[test]
fn assembling_machine_has_three_roles() {
    let data = load_fixture();

    // 同一原型在三个键中出现：item / recipe / assembling-machine
    // （机器本身是可制造物品，其配方名 = 物品名）
    let item_json = proto(&data, "item", "assembling-machine-1");
    let recipe_json = proto(&data, "recipe", "assembling-machine-1");
    let machine_json = proto(&data, "assembling-machine", "assembling-machine-1");

    // 物品角色
    let item: ItemComponent =
        serde_json::from_value(item_json.clone()).expect("ItemComponent 反序列化失败");
    assert_eq!(item.stack_size, 50, "组装机可堆叠 50");

    // 配方角色
    let recipe: RecipeComponent =
        serde_json::from_value(recipe_json.clone()).expect("RecipeComponent 反序列化失败");
    assert!(!recipe.ingredients.is_empty(), "配方应有原料");
    // 2.0 的组装机配方 JSON 无 energy_required（走默认值），仅断言原料/产物

    // 制造机器角色（继承链上的 CraftingMachine 层）
    let machine: CraftingMachineComponent = serde_json::from_value(machine_json.clone())
        .expect("CraftingMachineComponent 反序列化失败");
    assert!(
        (machine.crafting_speed - 0.5).abs() < 1e-9,
        "组装机1 速度 0.5"
    );
    assert_eq!(
        machine.crafting_categories,
        vec!["crafting".to_string(), "advanced-crafting".to_string()],
    );
    assert_eq!(machine.module_slots, None, "2.0 组装机1 无插件槽");

    // 组装机特有层（AssemblingMachineComponent：fixed_recipe 等）
    let am: AssemblingMachineComponent = serde_json::from_value(machine_json.clone())
        .expect("AssemblingMachineComponent 反序列化失败");
    assert_eq!(
        am.fixed_recipe, "",
        "组装机无固定配方（Literal 默认空串锁定）"
    );
}

#[test]
fn furnace_shares_crafter_component() {
    let data = load_fixture();
    let machine_json = proto(&data, "furnace", "stone-furnace");

    // 炉子与组装机共享 CraftingMachineComponent（语义层归一化的基础）
    let machine: CraftingMachineComponent =
        serde_json::from_value(machine_json.clone()).expect("反序列化失败");
    assert!((machine.crafting_speed - 1.0).abs() < 1e-9, "石炉速度 1.0");

    // 2.0 的炉子没有 fixed_recipe 字段（FurnaceComponent 中不存在），
    // furnace 与 assembling-machine 在 2.0 的差异进一步缩小
    let furnace: FurnaceComponent =
        serde_json::from_value(machine_json.clone()).expect("FurnaceComponent 反序列化失败");
    let _ = furnace; // 空组件（FurnacePrototype 无自有属性）
}

#[test]
fn entity_health_layer_parses_physical_fields() {
    let data = load_fixture();
    let machine_json = proto(&data, "assembling-machine", "assembling-machine-1");

    // 实体物理字段在机器 JSON 中（dump 无顶层 entity 键），
    // max_health 属于继承链的 EntityWithHealth 层
    let entity: EntityWithHealthComponent = serde_json::from_value(machine_json.clone())
        .expect("EntityWithHealthComponent 反序列化失败");
    assert_eq!(entity.max_health, 300.0, "组装机1 血量");

    let entity_base: EntityComponent =
        serde_json::from_value(machine_json.clone()).expect("EntityComponent 反序列化失败");
    assert!(entity_base.collision_box.is_some(), "应有碰撞箱");
}

#[test]
fn module_is_item_subclass() {
    let data = load_fixture();
    let module_json = proto(&data, "module", "speed-module");

    let module: ModuleComponent =
        serde_json::from_value(module_json.clone()).expect("ModuleComponent 反序列化失败");
    assert_eq!(module.category, "speed", "速度插件类别");
    assert_eq!(module.tier, 1);
}

#[test]
fn fluid_parses_temperature_and_heat_capacity() {
    let data = load_fixture();
    let fluid_json = proto(&data, "fluid", "water");

    let fluid: FluidComponent =
        serde_json::from_value(fluid_json.clone()).expect("FluidComponent 反序列化失败");
    assert_eq!(fluid.default_temperature, 15.0, "水的默认温度");
    // heat_capacity 是 Energy 类型 → 自定义 EnergyAmount（原始字符串保真）
    assert!(fluid.heat_capacity.is_some(), "水应有热容");
    let cap = fluid.heat_capacity.unwrap();
    assert!(cap.amount > 0.0, "水热容应为正值: {:?}", cap.amount);
}

#[test]
fn recipe_categories_and_results_parse() {
    let data = load_fixture();
    let recipe_json = proto(&data, "recipe", "iron-plate");

    let recipe: RecipeComponent =
        serde_json::from_value(recipe_json.clone()).expect("反序列化失败");
    assert!(!recipe.ingredients.is_empty(), "铁板配方应有原料");
    assert!(!recipe.results.is_empty(), "铁板配方应有产物");
    // 2.0 起配方支持多类别（categories 复数）
    assert_eq!(
        recipe.categories.as_deref(),
        Some(&["smelting".to_string()][..]),
        "铁板配方类别"
    );
}

#[test]
fn ignored_visual_fields_do_not_break_parsing() {
    let data = load_fixture();
    // 视觉字段（icon/graphics_set/working_sound 等）被忽略集跳过，
    // serde 反序列化时未知字段自动忽略——解析必须成功且核心字段完好
    let machine_json = proto(&data, "assembling-machine", "assembling-machine-2");
    let machine: CraftingMachineComponent =
        serde_json::from_value(machine_json.clone()).expect("带视觉字段的机器反序列化失败");
    assert!(
        (machine.crafting_speed - 0.75).abs() < 1e-9,
        "组装机2 速度 0.75"
    );
}
