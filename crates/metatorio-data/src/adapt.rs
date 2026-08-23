//! Factorio 2.0 dump → 2.1 数据适配层。
//!
//! 仓库 schema 面向 2.1.x（prototype-api.json 2.1.14）。Factorio 2.0
//! 导出的 data-raw-dump.json 字段形态与 2.1 schema 有差异，直接
//! 反序列化会失败或漏读。本插件提供**就地规范化**（每个适配层一个
//! 独立函数，便于按需增删），把 2.0 形态改写为 2.1 schema 期望的形态。
//!
//! 差异来源：官方 schema 更新 commit 86317bd（wube/factorio-data
//! "added version 2.1.7"）对 prototype-api.json 的字段变更——从 schema
//! 变化推导数据改写规则，而不是对原版数据内容做特判。
//!
//! 已实现的适配层：
//! - [`adapt_recipe_categories`]：2.0 recipe 的 `category`（标量）与
//!   `additional_categories`（数组）在 2.1 合并为单个 `categories` 数组。
//! - [`adapt_recipe_product_probability`]：2.0 产物（`results` 数组元素）
//!   的 `probability`（标量）在 2.1 迁移为 `independent_probability`
//!   （产物概率信息改为 flatten 的 `probability_info` 结构）。
//!
//! 后续可在此追加新层（如 result/result_count → results 数组、
//! Lua map 形态 ingredients → 数组等），在 [`normalize_2_0_dump`] 中
//! 按序调用。

use serde_json::Value;

/// 只读检测 dump 是否需要适配（存在任意 2.0 特征）。
///
/// 当前特征：recipe 使用 `category`/`additional_categories`，或任一
/// 产物的概率用 2.0 的 `probability` 标量（2.1 用
/// `independent_probability`）。全量遍历 recipe 组，命中即返回 true；
/// 无 recipe 组或已是 2.1 形态返回 false。调用方据此决定是否克隆改写
/// （避免对 2.1 dump 做无谓的深拷贝）。
pub fn needs_adaptation(dump: &Value) -> bool {
    let Some(recipes) = dump
        .get("recipe")
        .and_then(Value::as_object)
    else {
        return false;
    };
    recipes.values().any(|recipe| {
        let Some(obj) = recipe.as_object() else {
            return false;
        };
        if obj.contains_key("category") || obj.contains_key("additional_categories") {
            return true;
        }
        obj.get("results")
            .and_then(Value::as_array)
            .is_some_and(|results| {
                results.iter().any(|product| {
                    product.as_object().is_some_and(|p| p.contains_key("probability"))
                })
            })
    })
}

/// 把 2.0 形态的 dump 规范化到 2.1 schema 可加载的形态。
///
/// 对每个受影响的顶层类型组做就地改写；已经是 2.1 形态的 dump
/// 经过本函数后保持不变（幂等）。新增适配层时在此按序调用。
pub fn normalize_2_0_dump(dump: &mut Value) {
    let Some(root) = dump.as_object_mut() else {
        return;
    };
    if let Some(recipes) = root.get_mut("recipe").and_then(Value::as_object_mut) {
        for recipe in recipes.values_mut() {
            adapt_recipe_categories(recipe);
            adapt_recipe_product_probability(recipe);
        }
    }
}

/// 适配层：recipe 类别字段合并。
///
/// 2.0：`category: "crafting"`（标量）+ `additional_categories: [...]`
/// （数组，可为空/缺失）；
/// 2.1：`categories: ["crafting", ...]`（单个数组，category 在前、
/// additional_categories 依次追加，去重）。
///
/// 已含 `categories` 的 2.1 形态配方不做改动（幂等）。
pub fn adapt_recipe_categories(recipe: &mut Value) {
    let Some(obj) = recipe.as_object_mut() else {
        return;
    };
    if obj.contains_key("categories") {
        // 2.1 形态：已是数组则直接返回（保持幂等）。
        if obj["categories"].is_array() {
            return;
        }
        // 非数组的 categories（异常形态）：以现有值作为首个类别。
    }

    let mut merged: Vec<String> = Vec::new();
    let mut push = |name: &str, value: &Value| {
        match value {
            Value::String(category) => {
                if !merged.contains(category) {
                    merged.push(category.clone());
                }
            }
            Value::Array(list) => {
                for entry in list {
                    if let Value::String(category) = entry {
                        if !merged.contains(category) {
                            merged.push(category.clone());
                        }
                    }
                }
            }
            _ => {}
        }
        let _ = name;
    };

    if let Some(category) = obj.remove("category") {
        push("category", &category);
    }
    if let Some(additional) = obj.remove("additional_categories") {
        push("additional_categories", &additional);
    }

    if !merged.is_empty() {
        obj.insert(
            "categories".to_string(),
            Value::Array(merged.into_iter().map(Value::String).collect()),
        );
    }
}

/// 适配层：产物概率字段迁移。
///
/// 2.0：`results` 数组元素含 `probability: 0.5`（标量）；
/// 2.1：改为 `probability_info`（flatten）结构下的
/// `independent_probability`（`shared_probability` 是另一组字段，
/// 缺省 min=0/max=1）。2.0 标量映射到 `independent_probability`。
///
/// 只处理含 `probability` 且不含 `independent_probability` 的产物；
/// 已是 2.1 形态的产物不动（幂等）。
pub fn adapt_recipe_product_probability(recipe: &mut Value) {
    let Some(obj) = recipe.as_object_mut() else {
        return;
    };
    let Some(results) = obj.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for product in results {
        let Some(product_obj) = product.as_object_mut() else {
            continue;
        };
        if product_obj.contains_key("independent_probability") {
            continue; // 已是 2.1 形态
        }
        if let Some(probability) = product_obj.remove("probability") {
            product_obj.insert("independent_probability".to_string(), probability);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_and_additional_merge_into_categories() {
        let mut recipe = serde_json::json!({
            "type": "recipe", "name": "iron-plate",
            "category": "smelting",
            "additional_categories": ["crafting", "chemistry"],
            "energy_required": 1
        });
        adapt_recipe_categories(&mut recipe);
        assert_eq!(
            recipe["categories"],
            serde_json::json!(["smelting", "crafting", "chemistry"]),
            "category 在前、additional_categories 依次追加: {recipe}"
        );
        assert!(recipe.get("category").is_none(), "原 category 键应移除");
        assert!(
            recipe.get("additional_categories").is_none(),
            "原 additional_categories 键应移除"
        );
    }

    #[test]
    fn category_only_becomes_single_categories() {
        let mut recipe = serde_json::json!({
            "type": "recipe", "name": "iron-plate",
            "category": "smelting",
            "energy_required": 1
        });
        adapt_recipe_categories(&mut recipe);
        assert_eq!(recipe["categories"], serde_json::json!(["smelting"]));
    }

    #[test]
    fn additional_categories_only_merges() {
        let mut recipe = serde_json::json!({
            "type": "recipe", "name": "foo",
            "additional_categories": ["crafting", "chemistry"],
            "energy_required": 1
        });
        adapt_recipe_categories(&mut recipe);
        assert_eq!(
            recipe["categories"],
            serde_json::json!(["crafting", "chemistry"])
        );
    }

    #[test]
    fn duplicate_categories_are_deduped() {
        let mut recipe = serde_json::json!({
            "type": "recipe", "name": "foo",
            "category": "crafting",
            "additional_categories": ["crafting", "smelting"],
            "energy_required": 1
        });
        adapt_recipe_categories(&mut recipe);
        assert_eq!(
            recipe["categories"],
            serde_json::json!(["crafting", "smelting"]),
            "重复类别应去重: {recipe}"
        );
    }

    #[test]
    fn already_21_categories_is_unchanged() {
        let mut recipe = serde_json::json!({
            "type": "recipe", "name": "foo",
            "categories": ["smelting", "crafting"],
            "energy_required": 1
        });
        adapt_recipe_categories(&mut recipe);
        assert_eq!(recipe["categories"], serde_json::json!(["smelting", "crafting"]));
    }

    #[test]
    fn product_probability_maps_to_independent_probability() {
        let mut recipe = serde_json::json!({
            "type": "recipe", "name": "yumako-processing",
            "results": [
                {"type": "item", "name": "yumako", "amount": 1},
                {"type": "item", "name": "yumako-seed", "amount": 1, "probability": 0.02}
            ]
        });
        adapt_recipe_product_probability(&mut recipe);
        let results = recipe["results"].as_array().unwrap();
        // 无概率的产物不动
        assert!(results[0].get("probability").is_none());
        assert!(results[0].get("independent_probability").is_none());
        // 有概率的产物：probability → independent_probability
        assert_eq!(results[1]["independent_probability"], 0.02);
        assert!(results[1].get("probability").is_none(), "原 probability 键应移除");
    }

    #[test]
    fn product_probability_is_idempotent_on_21_shape() {
        let mut recipe = serde_json::json!({
            "type": "recipe", "name": "yumako-processing",
            "results": [
                {"type": "item", "name": "yumako-seed", "amount": 1,
                 "independent_probability": 0.02}
            ]
        });
        let before = recipe.clone();
        adapt_recipe_product_probability(&mut recipe);
        assert_eq!(recipe, before, "2.1 形态产物不应改动");
    }

    #[test]
    fn needs_adaptation_detects_product_probability() {
        let twenty = serde_json::json!({
            "recipe": {
                "yumako-processing": {
                    "type": "recipe", "name": "yumako-processing",
                    "results": [
                        {"type": "item", "name": "yumako-seed", "amount": 1, "probability": 0.02}
                    ]
                }
            }
        });
        assert!(
            needs_adaptation(&twenty),
            "2.0 产物 probability 应触发适配"
        );
        let twenty_one = serde_json::json!({
            "recipe": {
                "yumako-processing": {
                    "type": "recipe", "name": "yumako-processing",
                    "results": [
                        {"type": "item", "name": "yumako-seed", "amount": 1,
                         "independent_probability": 0.02}
                    ]
                }
            }
        });
        assert!(!needs_adaptation(&twenty_one), "2.1 产物形态无需适配");
    }

    /// 端到端：2.0 概率产物经 normalize 后加载，概率信息生效
    /// （normalized_output 使用 independent_probability）。
    #[test]
    fn normalized_20_product_probability_loads() {
        let mut dump = serde_json::json!({
            "item": {
                "yumako": { "type": "item", "name": "yumako", "stack_size": 100 },
                "yumako-seed": { "type": "item", "name": "yumako-seed", "stack_size": 100 }
            },
            "recipe": {
                "yumako-processing": {
                    "type": "recipe", "name": "yumako-processing",
                    "category": "crafting",
                    "energy_required": 1,
                    "ingredients": [{"type": "item", "name": "yumako", "amount": 1}],
                    "results": [
                        {"type": "item", "name": "yumako", "amount": 2},
                        {"type": "item", "name": "yumako-seed", "amount": 1, "probability": 0.02}
                    ]
                }
            }
        });
        normalize_2_0_dump(&mut dump);
        let store = crate::store::PrototypeStore::load(&dump).expect("规范化后应可加载");
        let recipe = store
            .get(crate::store::PrototypeGroup::Recipe, "yumako-processing")
            .expect("配方应在仓库中");
        let component = recipe
            .component::<crate::generated_components::RecipeComponent>()
            .expect("配方组件");
        let seed = component
            .results
            .iter()
            .find_map(|product| match product {
                crate::types::Product::Item(item) if item.name == "yumako-seed" => Some(item),
                _ => None,
            })
            .expect("应找到 yumako-seed 产物");
        assert!(
            (seed.probability_info.independent_probability - 0.02).abs() < 1e-9,
            "independent_probability 应为 0.02: {:?}",
            seed.probability_info
        );
        // 无概率产物保持默认 1.0
        let yumako = component
            .results
            .iter()
            .find_map(|product| match product {
                crate::types::Product::Item(item) if item.name == "yumako" => Some(item),
                _ => None,
            })
            .expect("应找到 yumako 产物");
        assert!(
            (yumako.probability_info.independent_probability - 1.0).abs() < 1e-9,
            "无概率产物默认 1.0: {:?}",
            yumako.probability_info
        );
    }

    #[test]
    fn normalize_is_idempotent_on_21_dump() {
        let mut dump = serde_json::json!({
            "recipe": {
                "iron-plate": {
                    "type": "recipe", "name": "iron-plate",
                    "categories": ["smelting"],
                    "energy_required": 1,
                    "ingredients": [{"type": "item", "name": "iron-ore", "amount": 1}],
                    "results": [{"type": "item", "name": "iron-plate", "amount": 1}]
                }
            }
        });
        let before = dump.clone();
        normalize_2_0_dump(&mut dump);
        assert_eq!(dump, before, "2.1 形态应保持幂等");
    }

    #[test]
    fn needs_adaptation_detects_20_features() {
        let twenty = serde_json::json!({
            "recipe": {
                "iron-plate": {
                    "type": "recipe", "name": "iron-plate",
                    "category": "smelting",
                    "energy_required": 1
                }
            }
        });
        assert!(needs_adaptation(&twenty), "2.0 category 应触发适配");
        let twenty_one = serde_json::json!({
            "recipe": {
                "iron-plate": {
                    "type": "recipe", "name": "iron-plate",
                    "categories": ["smelting"],
                    "energy_required": 1
                }
            }
        });
        assert!(!needs_adaptation(&twenty_one), "2.1 形态无需适配");
        assert!(!needs_adaptation(&serde_json::json!({})), "空 dump 无需适配");
    }

    /// 端到端：规范化后的 2.0 形态 dump 能被 PrototypeStore 加载，
    /// 且 categories 合并生效。
    #[test]
    fn normalized_20_dump_loads() {
        let mut dump = serde_json::json!({
            "item": {
                "iron-plate": { "type": "item", "name": "iron-plate", "stack_size": 100 },
                "iron-ore": { "type": "item", "name": "iron-ore", "stack_size": 50 }
            },
            "recipe": {
                "iron-plate": {
                    "type": "recipe", "name": "iron-plate",
                    "category": "smelting",
                    "additional_categories": ["crafting"],
                    "energy_required": 1,
                    "ingredients": [{"type": "item", "name": "iron-ore", "amount": 1}],
                    "results": [{"type": "item", "name": "iron-plate", "amount": 1}]
                }
            },
            "assembling-machine": {
                "stone-furnace": {
                    "type": "assembling-machine", "name": "stone-furnace",
                    "crafting_categories": ["smelting", "crafting"], "crafting_speed": 1,
                    "energy_usage": "90kW",
                    "energy_source": { "type": "electric" }
                }
            }
        });
        normalize_2_0_dump(&mut dump);
        let store = crate::store::PrototypeStore::load(&dump).expect("规范化后应可加载");
        let recipe = store
            .get(crate::store::PrototypeGroup::Recipe, "iron-plate")
            .expect("配方应在仓库中");
        let component = recipe
            .component::<crate::generated_components::RecipeComponent>()
            .expect("配方组件");
        assert_eq!(
            component.categories.as_deref(),
            Some(&["smelting".to_string(), "crafting".to_string()][..]),
            "categories 应为合并结果: {:?}",
            component.categories
        );
    }
}
