//! 原型的纯检视辅助：不产生前端 DTO，返回原始/基础类型。
//!
//! 供 Tauri 的浏览层（catalog/prototype_detail/suggest）与未来纯 Rust GUI
//! 复用——Rust 侧 GUI 直接读这些方法取原型信息，无需二次转换。

use metatorio_data::store::{PrototypeGroup, PrototypeStore};
use metatorio_data::{
    CraftingMachineComponent, FluidComponent, ItemComponent, MiningDrillComponent,
    RecipeComponent, ResourceEntityComponent,
};

/// 物品的机制标签（伪类别，供前端按机制过滤选择器）：
/// spoilable / plantable / fuel / launchable。
pub fn item_tags(store: &PrototypeStore, name: &str) -> Vec<String> {
    let Some(record) = store.get(PrototypeGroup::Item, name) else {
        return Vec::new();
    };
    let Some(item) = record.component::<ItemComponent>() else {
        return Vec::new();
    };
    let mut tags = Vec::new();
    if item.spoil_result.as_deref().is_some_and(|result| !result.is_empty())
        || item.spoil_ticks.is_some()
    {
        tags.push("spoilable".to_string());
    }
    if item.plant_result.as_deref().is_some_and(|result| !result.is_empty()) {
        tags.push("plantable".to_string());
    }
    if !item.fuel_category.is_empty() || item.fuel_value.is_some() {
        tags.push("fuel".to_string());
    }
    if !item.rocket_launch_products.is_empty() {
        tags.push("launchable".to_string());
    }
    tags
}

/// 物品燃料信息（燃料类别 + 热值；非燃料 → 空/None）。
pub fn item_fuel_info(store: &PrototypeStore, name: &str) -> (String, Option<f64>) {
    let Some(item) = store
        .get(PrototypeGroup::Item, name)
        .and_then(|record| record.component::<ItemComponent>())
    else {
        return (String::new(), None);
    };
    (item.fuel_category.clone(), item.fuel_value.map(|v| v.amount))
}

/// 流体燃料信息（热值 >0 才有；类别为空——流体按热值视为燃料）。
pub fn fluid_fuel_info(store: &PrototypeStore, name: &str) -> (String, Option<f64>) {
    let Some(fluid) = store
        .get(PrototypeGroup::Fluid, name)
        .and_then(|record| record.component::<FluidComponent>())
    else {
        return (String::new(), None);
    };
    let value = fluid.fuel_value().amount;
    (String::new(), if value > 0.0 { Some(value) } else { None })
}

/// 流体的机制标签：fluid-fuel（有热值可燃烧）/ fluid-heat（有比热容可提热）。
pub fn fluid_tags(store: &PrototypeStore, name: &str) -> Vec<String> {
    let Some(record) = store.get(PrototypeGroup::Fluid, name) else {
        return Vec::new();
    };
    let Some(fluid) = record.component::<FluidComponent>() else {
        return Vec::new();
    };
    let mut tags = Vec::new();
    if fluid.fuel_value().amount > 0.0 {
        tags.push("fluid-fuel".to_string());
    }
    if fluid.heat_capacity().amount > 0.0 {
        tags.push("fluid-heat".to_string());
    }
    tags
}

/// 配方有效类别：空数组 = 默认 `["crafting"]`，不是"任意机器都能造"。
pub fn effective_recipe_categories(recipe: &RecipeComponent) -> Vec<String> {
    let categories = recipe.categories.clone().unwrap_or_default();
    if categories.is_empty() {
        vec!["crafting".to_string()]
    } else {
        categories
    }
}

/// 资源有效类别：空 = 默认 `"basic-solid"`。
pub fn effective_resource_category(resource: &ResourceEntityComponent) -> String {
    if resource.category.is_empty() {
        "basic-solid".to_string()
    } else {
        resource.category.clone()
    }
}

/// 机器有效插件槽位（基础 + 品质加成；制造机/采矿机）。
pub fn effective_module_slots(
    store: &PrototypeStore,
    machine_id: &str,
    machine_quality: &str,
) -> usize {
    let Some(entity) = store.entity(machine_id) else {
        return 0;
    };
    let base = entity
        .component::<CraftingMachineComponent>()
        .and_then(|machine| machine.module_slots)
        .or_else(|| {
            entity
                .component::<MiningDrillComponent>()
                .and_then(|drill| drill.module_slots)
        })
        .unwrap_or(0) as usize;
    let bonus = entity
        .component::<CraftingMachineComponent>()
        .and_then(|machine| machine.module_slots_quality_bonus.get(machine_quality))
        .copied()
        .unwrap_or(0) as usize;
    base + bonus
}

/// 有效产出概率 = 独立概率 × 共享概率区间宽（与 egui normalized_output 一致）。
pub fn effective_probability(
    independent: f64,
    shared_min: f64,
    shared_max: f64,
) -> f64 {
    independent * (shared_max - shared_min)
}

/// 数字去尾零（3.000 → 3，1.500 → 1.5）。
pub fn trim_number(value: f64) -> String {
    let mut text = format!("{value:.3}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

/// 表面条件文本：`property: ≥x` / `property: ≤x` / `property: x ~ y`。
pub fn surface_condition_text(condition: &metatorio_data::SurfaceCondition) -> String {
    let min = condition.min();
    let max = condition.max();
    let range = match (min, max) {
        (min, max) if min == f64::MIN && max == f64::MAX => "任意".to_string(),
        (_, max) if max == f64::MAX => format!("≥ {}", trim_number(min)),
        (min, _) if min == f64::MIN => format!("≤ {}", trim_number(max)),
        (min, max) => format!("{} ~ {}", trim_number(min), trim_number(max)),
    };
    format!("{}: {range}", condition.property)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_number_strips_trailing_zeros() {
        assert_eq!(trim_number(3.0), "3");
        assert_eq!(trim_number(1.5), "1.5");
        assert_eq!(trim_number(0.250), "0.25");
    }

    #[test]
    fn effective_resource_category_defaults_to_basic_solid() {
        let resource = ResourceEntityComponent {
            category: String::new(),
            ..Default::default()
        };
        assert_eq!(effective_resource_category(&resource), "basic-solid");
        let resource = ResourceEntityComponent {
            category: "calcite".to_string(),
            ..Default::default()
        };
        assert_eq!(effective_resource_category(&resource), "calcite");
    }
}
