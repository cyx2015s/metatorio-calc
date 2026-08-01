//! 生成器自检测试：验证从 schema 生成的代码结构与统计符合预期。
//!
//! 这些测试是"正确性锚点"：
//! - 版本锚定：schema 的 application_version / api_version 变化会在此暴露
//! - 继承链：关键原型（组装机/电炉/模块）的链结构符合游戏类层次
//! - 类型映射：关键字段的 Rust 类型映射正确（标量、Option、自定义类型）

use metatorio_data_codegen::{Config, Schema, config::DEFAULT_CONCERNED_TYPENAMES, generate};

/// schema 文件路径（相对 codegen crate 的 workspace 布局）。
fn schema_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../metatorio-data/schema/prototype-api.json")
}

fn load_schema() -> Schema {
    let json = std::fs::read_to_string(schema_path()).expect("schema 文件不存在");
    Schema::parse(&json).expect("schema 解析失败")
}

#[test]
fn schema_version_is_pinned() {
    let schema = load_schema();
    // 版本锚定：游戏升级或 schema 格式变化时，此测试失败提醒重新审视生成代码
    assert_eq!(schema.application, "factorio");
    assert_eq!(schema.application_version, "2.1.11");
    assert_eq!(schema.api_version, 6);
    assert_eq!(schema.stage, "prototype");
}

#[test]
fn schema_has_expected_shape() {
    let schema = load_schema();
    assert!(schema.prototypes.len() > 200, "prototypes: {}", schema.prototypes.len());
    assert!(schema.types.len() > 500, "types: {}", schema.types.len());
    assert!(schema.defines.len() > 20, "defines: {}", schema.defines.len());
}

#[test]
fn inheritance_chain_matches_game_hierarchy() {
    let schema = load_schema();
    let chain = |name: &str| {
        let p = schema.prototype(name).expect(name);
        schema
            .prototype_chain(p)
            .iter()
            .map(|x| x.base.name.as_str())
            .collect::<Vec<_>>()
    };

    // 组装机与电炉共享 CraftingMachinePrototype（语义层归一化的依据）
    assert_eq!(
        chain("AssemblingMachinePrototype"),
        vec![
            "AssemblingMachinePrototype",
            "CraftingMachinePrototype",
            "EntityWithOwnerPrototype",
            "EntityWithHealthPrototype",
            "EntityPrototype",
            "Prototype",
            "PrototypeBase",
        ]
    );
    assert!(chain("FurnacePrototype").contains(&"CraftingMachinePrototype"));
    // 模块是物品的子类（ModulePrototype → ItemPrototype）
    assert!(chain("ModulePrototype").contains(&"ItemPrototype"));
    // 矿机不是制造机
    assert!(!chain("MiningDrillPrototype").contains(&"CraftingMachinePrototype"));
}

#[test]
fn generation_stats_are_reasonable() {
    let schema = load_schema();
    let (code, stats) = generate(&schema, &Config::default());

    // 关注类型数（DEFAULT_CONCERNED_TYPENAMES 的数量）
    assert_eq!(stats.concerned_typenames, DEFAULT_CONCERNED_TYPENAMES.len());
    // 组件数：原型继承链组件 + 嵌套 struct 组件（死类型修剪后显著小于全量）
    assert!(stats.component_structs > 70, "组件数: {}", stats.component_structs);
    // 字段数：足以覆盖计算所需（死类型修剪后为实际引用字段）
    assert!(stats.fields > 500, "字段数: {}", stats.fields);
    // 忽略集生效：视觉/音频字段被跳过
    assert!(stats.skipped_fields > 150, "忽略字段数: {}", stats.skipped_fields);
    // 生成代码包含关键组件
    assert!(code.contains("pub struct CraftingMachineComponent"));
    assert!(code.contains("pub struct EntityComponent"));
    assert!(code.contains("pub struct ItemComponent"));
    assert!(code.contains("pub struct RecipeComponent"));
    // 自定义类型（Energy → crate::EnergyAmount）被使用
    assert!(code.contains("crate::types::EnergyAmount"), "自定义类型映射缺失");
}

#[test]
fn crafting_machine_fields_are_correct() {
    let schema = load_schema();
    let (code, _) = generate(&schema, &Config::default());

    // CraftingMachineComponent 的关键字段（schema 的 CraftingMachinePrototype 属性）
    // crafting_speed: double, optional: false → f64（非 Option）
    assert!(code.contains("pub crafting_speed: f64"), "crafting_speed 应为非 Option f64");
    // crafting_categories: array of RecipeCategoryID → Vec<String>
    assert!(
        code.contains("pub crafting_categories: Vec<String>"),
        "crafting_categories 应为 Vec<String>"
    );
    // module_slots: ItemStackIndex（uint32 别名）optional → Option<u32>
    assert!(code.contains("pub module_slots: Option<u16>"), "module_slots 应为 Option<u16>（ItemStackIndex→uint16）");
    // fixed_recipe: RecipeID（string 别名）optional，但 schema 有 Literal 默认 "" → 锁定为非 Option
    assert!(code.contains("pub fixed_recipe: String"), "fixed_recipe 应被默认值锁定为 String（Literal 默认空串）");
    // allowed_effects: EffectTypeLimitation（struct 类型）→ 生成的组件
    assert!(code.contains("pub allowed_effects: Option<crate::types::EffectTypeLimitation>"), "allowed_effects 应映射到手写 EffectTypeLimitation");
}

#[test]
fn prototype_chains_registry_is_complete() {
    let schema = load_schema();
    let (code, stats) = generate(&schema, &Config::default());

    assert!(code.contains("PROTOTYPE_CHAINS"));
    // 组装机在注册表中
    assert!(code.contains(r#"("assembling-machine", &["#));
    // 注册表条目数与关注类型数一致（所有关注 typename 都有对应原型）
    let entries = code.matches("&[").count();
    assert!(entries >= stats.concerned_typenames, "注册表条目: {entries}");
}
