//! 配置 → 虚拟流展开（第一版）。
//!
//! 把工厂配置（`[Mechanic]`）展开为原始变量（PrimVar）+ 流系数：
//! - 1 个配置可展开为多个代数变量（流体插值：温度区间两端各 1 个，2^k 组合）
//! - **顺序无关**：配置先按稳定键（序列化）排序再编号，用户拖动配置列表
//!   不改变展开结果 → 求解结果稳定不重算
//! - 变温流体用 `TempRecipe`（内部隐式多副本，添加时自动分裂）
//! - 第一版范围：RecipeMechanic 的物质流（物品/流体 + 流体热量），normal 品质、
//!   无加成、无能源流（机器能耗/模块效果/品质分布后续迭代）

use crate::{DualVar, IdWithQuality, Mechanic, NORMAL_QUALITY, RecipeMechanic};
use metatorio_data::store::PrototypeGroup;
use metatorio_data::{
    generated_components::RecipeComponent,
    types::{Ingredient, Product},
};

use crate::context::Context;
use crate::prim_var::{ConfigId, Expansion};
use crate::temp_flow::TempFlow;

/// 展开工厂配置为原始变量 + 流系数。
pub fn expand(mechanics: &[Mechanic], ctx: &Context) -> Expansion {
    // 顺序无关：按稳定键（序列化）排序后再编号
    let mut indexed: Vec<(String, &Mechanic)> = mechanics
        .iter()
        .map(|m| (serde_json::to_string(m).unwrap_or_default(), m))
        .collect();
    indexed.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = Expansion::default();
    for (config, (_, mechanic)) in indexed.iter().enumerate() {
        if let Mechanic::Recipe(m) = mechanic {
            expand_recipe(config, m, ctx, &mut out);
        }
        // 其余机制第一版未实现（后续迭代）
    }
    out
}

/// 配方组件展开：物质流 + 流体热量流。
///
/// 流体输入温度区间 → TempRecipe 自动分裂（2^k 组合端，每端 1 个 PrimVar）；
/// 变量语义：1 单位配方运行次数（速度/模块效果后续迭代）。
fn expand_recipe(config: ConfigId, m: &RecipeMechanic, ctx: &Context, out: &mut Expansion) {
    let Some(recipe) = ctx
        .prototype
        .get(PrototypeGroup::Recipe, &m.recipe.id)
        .and_then(|r| r.component::<RecipeComponent>())
    else {
        return;
    };

    let mut temp = TempFlow::new();

    // 输入（消耗）
    for ingredient in &recipe.ingredients {
        match ingredient {
            Ingredient::Item(item) => {
                temp.add(
                    DualVar::Item(IdWithQuality::new(&item.name, NORMAL_QUALITY)),
                    -(item.amount as f64),
                );
            }
            Ingredient::Fluid(fluid) => {
                let default = fluid_default_temperature(ctx, &fluid.name);
                let lo = fluid
                    .temperature
                    .or(fluid.minimum_temperature)
                    .unwrap_or(default);
                let hi = fluid
                    .temperature
                    .or(fluid.maximum_temperature)
                    .unwrap_or(default);
                temp.add_fluid(ctx, &fluid.name, -fluid.amount, lo, hi);
            }
        }
    }

    // 产物（产出；产物流体温度为配方固定值，单点不分裂）
    for result in &recipe.results {
        match result {
            Product::Item(item) => {
                temp.add(
                    DualVar::Item(IdWithQuality::new(&item.name, NORMAL_QUALITY)),
                    item.normalized_output().base,
                );
            }
            Product::Fluid(fluid) => {
                let base = fluid.normalized_output().base;
                let temp_out = fluid
                    .temperature
                    .unwrap_or_else(|| fluid_default_temperature(ctx, &fluid.name));
                temp.add_fluid(ctx, &fluid.name, base, temp_out, temp_out);
            }
        }
    }

    out.variables.extend(temp.into_variables(config));
}

/// 流体的默认温度（原型 FluidComponent.default_temperature，缺省 0）。
fn fluid_default_temperature(ctx: &Context, name: &str) -> f64 {
    ctx.prototype
        .get(PrototypeGroup::Fluid, name)
        .and_then(|r| r.component::<metatorio_data::generated_components::FluidComponent>())
        .map(|f| f.default_temperature)
        .unwrap_or(0.0)
}
