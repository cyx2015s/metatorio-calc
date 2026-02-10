use std::path::PathBuf;

use crate::factorio::Dict;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct UserContext {
    pub time_scale: TimeScale,

    // 指定的科技里程碑关闭时，这个节点的科技将被视为未解锁（即使它的前置科技都已解锁了），以此来模拟不同的科技树分支
    pub tech_milestones: Vec<String>,

    #[serde(skip)]
    pub accessible_prototypes: Dict<Dict<bool>>,

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
