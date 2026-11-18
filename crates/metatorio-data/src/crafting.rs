//! 供编辑、枚举与展开共用的配方输入能力判定。
use crate::{CraftingMachineComponent, RecipeComponent, types::Ingredient};

impl RecipeComponent {
    pub fn requires_fluid_input(&self) -> bool {
        self.ingredients
            .iter()
            .any(|ingredient| matches!(ingredient, Ingredient::Fluid(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fluid_recipe_requires_a_recipe_input_box_not_a_fuel_box() {
        let fluid: RecipeComponent = serde_json::from_value(
            json!({"ingredients":[{"type":"fluid","name":"water","amount":1}]}),
        )
        .unwrap();
        let solid: RecipeComponent = serde_json::from_value(
            json!({"ingredients":[{"type":"item","name":"ore","amount":1}]}),
        )
        .unwrap();
        for (boxes, expected) in [
            (json!([]), false),
            (json!([{}]), false),
            (json!([{"production_type":"none"}]), false),
            (json!([{"production_type":"output"}]), false),
            (json!([{"production_type":"input"}]), true),
            (json!([{"production_type":"input-output"}]), true),
        ] {
            let machine: CraftingMachineComponent=serde_json::from_value(json!({"fluid_boxes":boxes,"energy_source":{"type":"fluid","fluid_box":{"production_type":"input"}}})).unwrap();
            assert_eq!(machine.supports_recipe_fluid_input(&fluid), expected);
            assert!(machine.supports_recipe_fluid_input(&solid));
        }
    }
}

impl CraftingMachineComponent {
    /// 只统计配方自身的流体槽；流体能源属于独立的燃料系统。
    /// 原型默认 `production_type` 为 `"none"`，而非 `"input"`。
    pub fn supports_fluid_input(&self) -> bool {
        self.fluid_boxes.iter().any(|fluid_box| {
            matches!(
                fluid_box.production_type.as_deref(),
                Some("input" | "input-output")
            )
        })
    }

    pub fn supports_recipe_fluid_input(&self, recipe: &RecipeComponent) -> bool {
        !recipe.requires_fluid_input() || self.supports_fluid_input()
    }
}
