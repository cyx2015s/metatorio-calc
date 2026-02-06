#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct UserContext {
    pub time_scale: TimeScale,

    pub milestones: Vec<DependencyItem>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeScale {
    #[default]
    Seconds,
    Minutes,
    Hours,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum DependencyItem {
    Item(String),
    Fluid(String),
    Entity(String),
    Technology(String),
    Location(String),
    Recipe(String),
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Relation {
    UnlockedByTechnology,
    RequiredByMachine,
    RequiredAsIngredient,
    ProducedAsResult,
}

pub type DependencyGraph = petgraph::Graph<DependencyItem, Relation>;
