// 一个 planner 下可以打开多个 project
// 一个 project 下可以打开多个 factory

use std::path::PathBuf;

use crate::factorio::planner::PlannerView;

pub enum SelectedPage {
    Planner(usize), // 规划的工厂
    Settings,       // 项目偏好设置
}

pub struct ProjectView {
    pub name: String,
    pub file_path: Option<PathBuf>,
    pub plannings: Vec<PlannerView>,
    pub selected_planning: Option<usize>,
}
