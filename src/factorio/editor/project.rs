use crate::factorio::planner::ProjectInstance;

pub struct ContextView {
    pub name: String,
    pub projects: Vec<ProjectInstance>,
    pub selected_page: Option<usize>,
}
