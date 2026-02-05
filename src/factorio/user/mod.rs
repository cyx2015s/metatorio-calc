use std::sync::Arc;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct FactorioPreferences {
    pub time_scale: TimeScale,

    pub milestones: Vec<DependencyId>,
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
pub enum DependencyId {
    Item(String),
    Fluid(String),
    Entity(String),
    Technology(String),
    Location(String),
    Recipe(String),
}

pub struct DependencyNode {
    pub id: DependencyId,
    pub link: DependencyLink,
}

#[derive(Debug, Clone)]
pub enum DependencyLink {
    Phony,                           // 总是可用
    AnyOf(Vec<Arc<DependencyLink>>), // 任意一个可用，自身可用
    AllOf(Vec<Arc<DependencyLink>>), // 全部都可用，自身才可用
}
