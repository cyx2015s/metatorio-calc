use std::{collections::HashMap, hash::Hash, path::PathBuf};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct UserContext {
    pub time_scale: TimeScale,

    pub milestones: Vec<DependencyItem>,

    #[serde(skip)]
    pub saved: bool,
    #[serde(skip)]
    pub file_path: Option<PathBuf>,
    #[serde(skip)]
    pub selected_page: ProjectPage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TimeScale {
    #[default]
    Seconds,
    Minutes,
    Hours,
}

impl TimeScale {
    pub fn multiplier(&self) -> f64 {
        match self {
            TimeScale::Seconds => 1.0,
            TimeScale::Minutes => 60.0,
            TimeScale::Hours => 3600.0,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectPage {
    Index(usize), // 工厂设置页面
    #[default]
    UserContext, // 偏好设置页面
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
#[non_exhaustive]
pub enum DependencyType {
    #[default]
    Item,
    Fluid,
    Entity,
    Technology,
    Location,
    Recipe,
    Quality,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct DependencyItem {
    pub id: String,
    pub category: DependencyType,
}

impl DependencyItem {
    pub fn new(id: impl Into<String>, category: DependencyType) -> Self {
        Self {
            id: id.into(),
            category,
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub enum Relation {
    // 有多个同类型的依赖节点时，必须全部解锁才能解锁自身
    #[default]
    AllOfSame,
    // 有多个同类型的依赖节点时，只需解锁其中一个即可解锁自身
    OneOfSame,
}

#[derive(Debug, Clone, Default)]
pub struct DependencyGraph<N, E> {
    pub graph: petgraph::Graph<N, E, petgraph::Directed, u32>,
    pub indices: HashMap<N, petgraph::graph::NodeIndex<u32>>,
}

impl<N, E> DependencyGraph<N, E>
where
    N: Eq + Hash + Clone,
{
    pub fn new() -> Self {
        Self {
            graph: petgraph::Graph::new(),
            indices: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, node: N) {
        if !self.indices.contains_key(&node) {
            let index = self.graph.add_node(node.clone());
            self.indices.insert(node, index);
        }
    }

    pub fn add_edge(&mut self, from: &N, to: &N, edge: E) {
        if let (Some(&from_idx), Some(&to_idx)) = (self.indices.get(from), self.indices.get(to)) {
            self.graph.add_edge(from_idx, to_idx, edge);
        }
    }
}
