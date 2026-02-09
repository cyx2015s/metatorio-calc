use crate::{
    concept::{EditorView, SolveContext},
    factorio::{FactorioContext, GenericItem},
};

#[derive(Debug, Clone)]
pub struct UserContextEditor {}

impl SolveContext for UserContextEditor {
    type Game = FactorioContext;
    type Item = GenericItem;
}

impl EditorView for UserContextEditor {
    fn editor_view(&mut self, ui: &mut egui::Ui, _game: &Self::Game) -> bool {
        ui.label("用户设置编辑器");
        false
    }
}
