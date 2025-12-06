use std::{collections::HashMap, fmt::Display, fs::read_to_string, path::Path, thread::sleep, time::{Duration, Instant}};

use good_lp::{Expression, ProblemVariables, Solution, SolverModel, VariableDefinition, microlp};
use serde_json::*;

mod lp;
mod relation;
mod types;
mod context;

use crate::types::{CraftingMachinePrototype, RecipePrototype};

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
enum Material {
    Item(String),
    Fluid(String),
}

impl Display for Material {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Material::Item(name) => write!(f, "Item({})", name),
            Material::Fluid(name) => write!(f, "Fluid({})", name),
        }
    }
}

fn main() {
    dotenv::dotenv().ok();
    let path = Path::new(std::env::var("FACTORIO_PATH").unwrap().as_str())
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("script-output/data-raw-dump__.json");
    // println!("{}", path.to_str().unwrap());
    let data_raw_dump: Value = from_str(read_to_string(path).unwrap().as_str()).unwrap();
    let production_target = (Material::Item("rocket-part".to_string()), 10.0); // 10/s
    let mut recipes = HashMap::new();
    let mut recipe_dict = HashMap::new();
    let mut items_dict = HashMap::new();
    let mut items_sources = HashMap::new();
    let mut problem = ProblemVariables::new();
    println!(
        "There are {} recipes. ",
        data_raw_dump["recipe"].as_object().unwrap().len()
    );
    for (name, value) in data_raw_dump["recipe"].as_object().unwrap().iter() {
        // println!("{:?}", name);
        let value: RecipePrototype = serde_json::from_value(value.clone()).unwrap();
        // println!("{:#?}", value);
        let recipe_variable = problem.add(
            VariableDefinition::new()
                .min(0.0)
                .name(name.clone())
                .initial(0.0),
        );
        // Store ingredients and results
        for ingredient in &value.ingredients {
            match ingredient {
                types::RecipeIngredient::item(item_ingredient) => {
                    items_dict
                        .entry(Material::Item(item_ingredient.name.clone()))
                        .or_insert_with(Vec::new)
                        .push((
                            recipe_variable,
                            -(item_ingredient.amount as f64 / value.energy_required),
                        ));
                }
                types::RecipeIngredient::fluid(fluid_ingredient) => {
                    items_dict
                        .entry(Material::Fluid(fluid_ingredient.name.clone()))
                        .or_insert_with(Vec::new)
                        .push((
                            recipe_variable,
                            -(fluid_ingredient.amount as f64 / value.energy_required),
                        ));
                }
            }
        }
        for result in &value.results {
            match result {
                types::RecipeResult::item(item_result) => {
                    items_dict
                        .entry(Material::Item(item_result.name.clone()))
                        .or_insert_with(Vec::new)
                        .push((
                            recipe_variable,
                            item_result.normalized_output().0 as f64 / value.energy_required,
                        ));
                }
                types::RecipeResult::fluid(fluid_result) => {
                    items_dict
                        .entry(Material::Fluid(fluid_result.name.clone()))
                        .or_insert_with(Vec::new)
                        .push((
                            recipe_variable,
                            fluid_result.normalized_output().0 as f64 / value.energy_required,
                        ));
                }
            }
        }
        recipe_dict.insert(name.clone(), recipe_variable);
        recipes.insert(recipe_variable, value);
    }
    // 给所有没有产出的物品添加伪源
    for (material, usages) in &items_dict {
        // println!("Material: {}", material);
        if usages.iter().all(|count| count.1 < 0.0) {
            // No recipe can produce this material
            let item_source_variable = problem.add(
                VariableDefinition::new()
                    .min(0.0)
                    .name(format!("|伪源|{material}"))
                    .initial(0.0),
            );
            items_sources.insert(material.clone(), item_source_variable);
        }
    }
    // 优化目标：降低物品消耗
    let mut optimization_target = items_sources
        .iter()
        .fold(Expression::from(0.0), |acc, (_material, var)| acc + *var);

    optimization_target += recipe_dict
        .iter()
        .fold(Expression::from(0.0), |acc, (_name, var)| acc + *var * 0.1);

    let mut constraints = vec![];
    // 产量约束，大于一定值
    let production_expr = items_dict
        .get(&production_target.0)
        .unwrap()
        .iter()
        .fold(Expression::from(0.0), |acc, (var, coeff)| {
            acc + (*var * (*coeff))
        });
    constraints.push(production_expr.geq(production_target.1));

    for (material, usages) in &items_dict {
        if material == &production_target.0 {
            continue;
        }
        let material_expression = usages
            .iter()
            .fold(Expression::from(0.0), |acc, (var, coeff)| {
                acc + (*var * (*coeff))
            });
        let source_variable = items_sources.get(material);
        let source_expression: Expression = match source_variable {
            Some(var) => *var * 1.0,
            None => Expression::from(0.0),
        };
        constraints.push((material_expression.clone() + source_expression).geq(0.0));
    }

    let model = problem
        .minimise(&optimization_target)
        .using(microlp)
        .with_all(constraints);

    let now = Instant::now();
    let solution = model.solve();
    let delta = now.elapsed();
    if solution.is_err() {
        println!("求解失败: {:?}", solution.err().unwrap());
        return;
    }
    let solution = solution.unwrap();
    println!("求解成功，耗时: {:?}", delta);

    println!(
        "生产目标：{:?} 每秒 {}",
        production_target.0, production_target.1
    );
    println!("优化结果: {}", solution.eval(&optimization_target));
    println!("假定机器的速度为1，没有其他加成");
    for (name, var) in &recipe_dict {
        let value = solution.value(*var);
        if value > 1e-5 {
            println!("配方 {} 机器个数: {}", name, value);
        }
    }
    for (material, var) in &items_sources {
        let value = solution.value(*var);
        if value > 1e-5 {
            println!("物品 {} 伪源供应速率: {} /s", material, value);
        } else {
            println!("物品 {} 无需伪源供应", material);
        }
    }

    // for (name, value) in data_raw_dump["assembling-machine"]
    //     .as_object()
    //     .unwrap()
    //     .iter()
    // {
    //     println!("{:?}", name);
    //     let value: CraftingMachinePrototype = serde_json::from_value(value.clone()).unwrap();
    //     println!("{:#?}", value);
    // }

    // for (name, value) in data_raw_dump["furnace"].as_object().unwrap().iter() {
    //     println!("{:?}", name);
    //     let value: CraftingMachinePrototype = serde_json::from_value(value.clone()).unwrap();
    //     println!("{:#?}", value);
    // }
    sleep(Duration::from_millis(10000));
}
