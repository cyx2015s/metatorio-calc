use crate::factorio::Dict;

pub struct PlantPrototype {
    pub growth_ticks: f64,
    pub harvest_emmisions: Dict<f64>,
}
