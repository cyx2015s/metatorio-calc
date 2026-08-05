//! 配置 → 虚拟流展开（第一版）。
//!
//! 把工厂配置（`[Mechanic]`）展开为原始变量（PrimVar）+ 流系数：
//! - 1 个配置可展开为多个代数变量（流体插值：温度区间两端各 1 个，2^k 组合）
//! - **顺序无关**：配置先按稳定键（序列化）排序再编号，用户拖动配置列表
//!   不改变展开结果 → 求解结果稳定不重算
//! - 第一版范围：RecipeMechanic 的物质流（物品/流体 + 流体热量），normal 品质、
//!   无加成、无能源流（机器能耗/模块效果/品质分布后续迭代）

use crate::{
    DualVar, IdWithQuality, Mechanic, NORMAL_QUALITY, RecipeMechanic,
};
use metatorio_data::store::PrototypeGroup;
use metatorio_data::{
    generated_components::{FluidComponent, RecipeComponent},
    types::{Ingredient, Product},
};

use crate::context::Context;
use crate::prim_var::{ConfigId, ExpandedVariable, Expansion, Flow, PrimVar, Variant};

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
        match mechanic {
            Mechanic::Recipe(m) => expand_recipe(config, m, ctx, &mut out),
            // 其余机制第一版未实现（后续迭代）
            _ => {}
        }
    }
    out
}

/// 配方组件展开：物质流 + 流体热量流。
///
/// 流体输入温度区间 → 2^k 组合端（每端 1 个 PrimVar）；k = 流体原料数。
/// 变量语义：1 单位配方运行次数（速度/模块效果后续迭代）。
fn expand_recipe(config: ConfigId, m: &RecipeMechanic, ctx: &Context, out: &mut Expansion) {
    let Some(recipe) = ctx
        .prototype
        .get(PrototypeGroup::Recipe, &m.recipe.id)
        .and_then(|r| r.component::<RecipeComponent>())
    else {
        return;
    };

    // 物品输入（所有组合端共享）
    let mut item_inputs: Vec<(String, f64)> = Vec::new();
    // 流体输入：名字 → (数量, 温度区间 [lo, hi])
    let mut fluid_inputs: Vec<(String, f64, [f64; 2])> = Vec::new();
    for ingredient in &recipe.ingredients {
        match ingredient {
            Ingredient::Item(item) => {
                item_inputs.push((item.name.clone(), f64::from(item.amount)));
            }
            Ingredient::Fluid(f) => {
                // 配方未指定温度 → 用流体默认温度（单点，无插值）
                let default = fluid_default_temperature(ctx, &f.name);
                let lo = f
                    .temperature
                    .or(f.minimum_temperature)
                    .unwrap_or(default);
                let hi = f.temperature.or(f.maximum_temperature).unwrap_or(default);
                fluid_inputs.push((f.name.clone(), f.amount, [lo, hi]));
            }
        }
    }

    // 物品/流体产物（所有组合端共享）
    let mut item_outputs: Vec<(String, f64)> = Vec::new();
    let mut fluid_outputs: Vec<(String, f64, f64)> = Vec::new();
    for result in &recipe.results {
        match result {
            Product::Item(item) => {
                item_outputs.push((item.name.clone(), item.normalized_output().base));
            }
            Product::Fluid(fluid) => {
                let base = fluid.normalized_output().base;
                let temp = fluid.temperature.unwrap_or_else(|| {
                    fluid_default_temperature(ctx, &fluid.name)
                });
                fluid_outputs.push((fluid.name.clone(), base, temp));
            }
        }
    }

    // 组合端：k 个流体输入 → 2^k 个代数变量
    let k = fluid_inputs.len();
    let combos = if k == 0 { 1 } else { 1usize << k.min(8) };
    for mask in 0..combos {
        let mut flow: Flow = Default::default();
        // 物品输入（消耗）
        for (name, amount) in &item_inputs {
            add(&mut flow, DualVar::Item(IdWithQuality::new(name, NORMAL_QUALITY)), -*amount);
        }
        // 流体输入（消耗）+ 流体热量（消耗）
        for (i, (name, amount, [lo, hi])) in fluid_inputs.iter().enumerate() {
            let temp = if mask & (1 << i) != 0 { *hi } else { *lo };
            add(&mut flow, DualVar::Fluid { name: name.clone() }, -*amount);
            add(
                &mut flow,
                DualVar::FluidHeat {
                    filter: name.clone(),
                },
                -fluid_heat(ctx, name, *amount, temp),
            );
        }
        // 物品产物（产出）
        for (name, amount) in &item_outputs {
            add(&mut flow, DualVar::Item(IdWithQuality::new(name, NORMAL_QUALITY)), *amount);
        }
        // 流体产物（产出）+ 流体热量（产出）
        for (name, amount, temp) in &fluid_outputs {
            add(&mut flow, DualVar::Fluid { name: name.clone() }, *amount);
            add(
                &mut flow,
                DualVar::FluidHeat {
                    filter: name.clone(),
                },
                fluid_heat(ctx, name, *amount, *temp),
            );
        }
        let variant = if k == 0 {
            Variant::Single
        } else {
            Variant::Interp(mask as u8)
        };
        out.variables.push(ExpandedVariable {
            prim_var: PrimVar { config, variant },
            flow,
        });
    }
}

/// 流体热量：amount × (温度 - 默认温度) × 比热容。
fn fluid_heat(ctx: &Context, name: &str, amount: f64, temperature: f64) -> f64 {
    let Some(fluid) = ctx.prototype.get(PrototypeGroup::Fluid, name) else {
        return 0.0;
    };
    let Some(component) = fluid.component::<FluidComponent>()
    else {
        return 0.0;
    };
    amount * (temperature - component.default_temperature) * component.heat_capacity().amount
}

/// 流体的默认温度（配方未指定温度时的输入/产出温度）。
fn fluid_default_temperature(ctx: &Context, name: &str) -> f64 {
    ctx.prototype
        .get(PrototypeGroup::Fluid, name)
        .and_then(|r| r.component::<FluidComponent>())
        .map(|f| f.default_temperature)
        .unwrap_or(0.0)
}

/// 累加流系数。
fn add(flow: &mut Flow, key: DualVar, value: f64) {
    if value != 0.0 {
        *flow.entry(key).or_insert(0.0) += value;
    }
}
