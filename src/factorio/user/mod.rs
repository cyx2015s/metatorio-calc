mod graph;

use crate::concept::AIndexMap;
pub use graph::*;

use std::{path::PathBuf, sync::mpsc::Sender};

use crate::factorio::{Dict, planner::FactoryInstance};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ProjectContext {
    pub time_scale: TimeScale,

    // 指定的科技里程碑关闭时，这个节点的科技将被视为未解锁（即使它的前置科技都已解锁了），以此来模拟不同的科技树分支
    pub tech_milestones: Vec<(String, bool)>,

    pub recipe_productivity: AIndexMap<String, f64>,

    pub ignore_productivity: bool,

    pub mining_productivity: f64,

    pub all_accessible: bool,
    #[serde(skip)]
    pub accessible_technologies: Vec<String>,

    #[serde(skip)]
    pub accessible_prototypes: Dict<Dict<bool>>,

    #[serde(skip)]
    pub cur_max_quality_level: u8,

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

    #[serde(skip)]
    pub milestone_graph: AIndexMap<String, MilestoneNode>,

    #[serde(skip)]
    pub hovered_tech: Option<String>,
}

impl ProjectContext {
    pub fn with_factory_sender(mut self, sender: Sender<FactoryInstance>) -> Self {
        self.factory_sender = Some(sender);
        self
    }

    pub fn is_prototype_accessible(&self, category: &str, name: &str) -> bool {
        if self.all_accessible {
            return true;
        }
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

    pub fn get_recipe_productivity(&self, recipe_name: &str) -> Option<f64> {
        if self.ignore_productivity {
            return None;
        }
        self.recipe_productivity.get(recipe_name).cloned()
    }

    pub fn get_mining_productivity(&self) -> f64 {
        self.mining_productivity
    }

    pub fn max_quality(&self) -> u8 {
        if self.all_accessible {
            self.max_quality_level
        } else {
            self.cur_max_quality_level
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
