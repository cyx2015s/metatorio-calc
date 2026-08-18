//! allowed_modules 命令核心逻辑的真实 dump 验证：
//! 装配机-1 + electronic-circuit 配方 → 应允许 speed/efficiency 插件。
use metatorio_data::store::{PrototypeGroup, PrototypeStore};
use metatorio_data::types::EffectTypeLimitation;
use metatorio_data::{CraftingMachineComponent, ModuleComponent, RecipeComponent};

fn real_store() -> PrototypeStore {
    let path = "C:\\Users\\mirac\\AppData\\Roaming\\Factorio\\script-output\\data-raw-dump.json";
    if !std::path::Path::new(path).exists() {
        panic!("无真实 dump");
    }
    let raw = std::fs::read(path).expect("读 dump");
    let dump: serde_json::Value = serde_json::from_slice(&raw).expect("解析 dump");
    PrototypeStore::load(&dump).expect("dump 加载失败")
}

fn module_allowed(
    module: &ModuleComponent,
    machine_categories: &Option<Vec<String>>,
    machine_allowed_effects: &Option<EffectTypeLimitation>,
    recipe: Option<&RecipeComponent>,
) -> bool {
    // 与 auto_plan::module_allowed 相同的逻辑（此处独立复制便于验证命令路径）。
    if let Some(categories) = machine_categories {
        if !categories.is_empty() && !categories.contains(&module.category) {
            return false;
        }
    }
    let recipe_allowed = |kind: metatorio_data::types::EffectType, recipe_allow: bool| {
        recipe_allow && machine_allowed_effects.is_none_or(|limits| limits[kind])
    };
    let effect = &module.effect;
    if effect.speed > 0.0
        && !recipe_allowed(
            metatorio_data::types::EffectType::Speed,
            recipe.is_none_or(|r| r.allow_speed),
        )
    {
        return false;
    }
    if effect.productivity > 0.0
        && !recipe_allowed(
            metatorio_data::types::EffectType::Productivity,
            recipe.is_none_or(|r| r.allow_productivity),
        )
    {
        return false;
    }
    if effect.quality > 0.0
        && !recipe_allowed(
            metatorio_data::types::EffectType::Quality,
            recipe.is_none_or(|r| r.allow_quality),
        )
    {
        return false;
    }
    if effect.consumption < 0.0
        && !recipe_allowed(
            metatorio_data::types::EffectType::Consumption,
            recipe.is_none_or(|r| r.allow_consumption),
        )
    {
        return false;
    }
    if effect.pollution < 0.0
        && !recipe_allowed(
            metatorio_data::types::EffectType::Pollution,
            recipe.is_none_or(|r| r.allow_pollution),
        )
    {
        return false;
    }
    true
}

#[test]
fn real_dump_machine_with_recipe_allows_modules() {
    let store = real_store();
    let Some(record) = store.get(PrototypeGroup::Entity, "assembling-machine-1") else {
        panic!("装配机-1 不在仓库");
    };
    let Some(machine) = record.component::<CraftingMachineComponent>() else {
        panic!("装配机-1 无 CraftingMachineComponent");
    };
    let recipe = store
        .get(PrototypeGroup::Recipe, "electronic-circuit")
        .and_then(|r| r.component::<RecipeComponent>())
        .expect("electronic-circuit 配方存在");
    let mut allowed: Vec<String> = Vec::new();
    for item_record in store.group(PrototypeGroup::Item) {
        let Some(module) = item_record.component::<ModuleComponent>() else {
            continue;
        };
        if module_allowed(
            module,
            &machine.allowed_module_categories,
            &machine.allowed_effects,
            Some(&recipe),
        ) {
            allowed.push(item_record.name.clone());
        }
    }
    eprintln!("装配机-1 + electronic-circuit 允许的插件: {allowed:?}");
    assert!(
        allowed.iter().any(|name| name.starts_with("speed-module")),
        "应允许速度插件：{allowed:?}"
    );
    assert!(
        allowed.iter().any(|name| name.starts_with("efficiency-module")),
        "应允许效率插件：{allowed:?}"
    );
}
