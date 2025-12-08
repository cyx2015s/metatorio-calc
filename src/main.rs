use std::{collections::HashMap, fmt::Display, fs::read_to_string, path::Path, thread::sleep, time::{Duration, Instant}};

use good_lp::{Expression, ProblemVariables, Solution, SolverModel, VariableDefinition, microlp};
use serde_json::*;

mod lp;
mod relation;
mod context;
mod ctx;

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

}
