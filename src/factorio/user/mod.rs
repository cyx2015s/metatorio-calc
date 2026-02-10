use std::{
    path::PathBuf,
    sync::mpsc::Sender,
};

use crate::
    factorio::{Dict, planner::FactoryInstance}
;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct UserContext {
    pub time_scale: TimeScale,

    // 指定的科技里程碑关闭时，这个节点的科技将被视为未解锁（即使它的前置科技都已解锁了），以此来模拟不同的科技树分支
    #[serde(skip)]
    pub tech_milestones: Vec<(String, bool)>,

    #[serde(skip)]
    pub accessible_prototypes: Dict<Dict<bool>>,

    #[serde(skip)]
    pub saved: bool,
    #[serde(skip)]
    pub file_path: Option<PathBuf>,
    #[serde(skip)]
    pub selected_page: ProjectPage,

    #[serde(skip)]
    pub factory_sender: Option<Sender<FactoryInstance>>,
}

impl Default for UserContext {
    fn default() -> Self {
        Self {
            time_scale: TimeScale::Seconds,
            tech_milestones: Vec::new(),
            accessible_prototypes: Dict::new(),
            saved: true,
            file_path: None,
            selected_page: ProjectPage::default(),
            factory_sender: None,
        }
    }
}

impl Clone for UserContext {
    fn clone(&self) -> Self {
        Self {
            time_scale: self.time_scale,
            tech_milestones: self.tech_milestones.clone(),
            accessible_prototypes: self.accessible_prototypes.clone(),
            saved: self.saved,
            file_path: self.file_path.clone(),
            selected_page: self.selected_page,
            factory_sender: self.factory_sender.clone(),
            ..Default::default()
        }
    }
}

impl UserContext {
    pub fn with_factory_sender(mut self, sender: Sender<FactoryInstance>) -> Self {
        self.factory_sender = Some(sender);
        self
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
