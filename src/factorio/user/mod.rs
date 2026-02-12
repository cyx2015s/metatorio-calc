mod graph;

pub use graph::*;
use indexmap::IndexMap;

use std::{path::PathBuf, sync::mpsc::Sender};

use crate::factorio::{Dict, planner::FactoryInstance};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ProjectContext {
    pub time_scale: TimeScale,

    // 指定的科技里程碑关闭时，这个节点的科技将被视为未解锁（即使它的前置科技都已解锁了），以此来模拟不同的科技树分支
    pub tech_milestones: Vec<(String, bool)>,

    pub recipe_productivity: IndexMap<String, f64>,

    pub mining_productivity: f64,

    #[serde(skip)]
    pub accessible_technologies: Vec<String>,

    #[serde(skip)]
    pub accessible_prototypes: Dict<Dict<bool>>,

    #[serde(skip)]
    pub max_quality_level: u8,

    #[serde(skip)]
    pub saved: bool,
    #[serde(skip)]
    pub file_path: Option<PathBuf>,
    #[serde(skip)]
    pub selected_page: ProjectPage,

    #[serde(skip)]
    pub factory_sender: Option<Sender<FactoryInstance>>,
}

impl Default for ProjectContext {
    fn default() -> Self {
        Self {
            time_scale: TimeScale::Seconds,
            tech_milestones: Vec::new(),
            accessible_technologies: Vec::new(),
            accessible_prototypes: Dict::new(),
            recipe_productivity: IndexMap::new(),
            max_quality_level: 0,
            saved: true,
            file_path: None,
            selected_page: ProjectPage::default(),
            factory_sender: None,
            mining_productivity: 0.0,
        }
    }
}

impl ProjectContext {
    pub fn with_factory_sender(mut self, sender: Sender<FactoryInstance>) -> Self {
        self.factory_sender = Some(sender);
        self
    }

    pub fn is_prototype_accessible(&self, category: &str, name: &str) -> bool {
        if let Some(category_dict) = self.accessible_prototypes.get(category) {
            if let Some(accessible) = category_dict.get(name) {
                *accessible
            } else {
                false
            }
        } else {
            true
        }
    }
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
