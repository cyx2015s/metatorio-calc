use std::{fs::read_to_string, path::Path};

use serde_json::*;

mod relation;
mod types;

use crate::types::{CraftingMachinePrototype, RecipePrototype};

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
    println!(
        "There are {} recipes. ",
        data_raw_dump["recipe"].as_object().unwrap().len()
    );
    for (name, value) in data_raw_dump["recipe"].as_object().unwrap().iter() {
        println!("{:?}", name);
        let value: RecipePrototype = serde_json::from_value(value.clone()).unwrap();
        println!("{:#?}", value);
    }

    for (name, value) in data_raw_dump["assembling-machine"]
        .as_object()
        .unwrap()
        .iter()
    {
        println!("{:?}", name);
        let value: CraftingMachinePrototype = serde_json::from_value(value.clone()).unwrap();
        println!("{:#?}", value);
    }

    for (name, value) in data_raw_dump["furnace"].as_object().unwrap().iter() {
        println!("{:?}", name);
        let value: CraftingMachinePrototype = serde_json::from_value(value.clone()).unwrap();
        println!("{:#?}", value);
    }
}
