use crate::{
    concept::{EditorView, SolveContext},
    factorio::{FactorioContext, GenericItem},
};

#[derive(Debug, Clone)]
pub struct UserContextEditor<'a> {
    pub game: &'a FactorioContext,
}

impl<'a> UserContextEditor<'a> {
    pub fn new(game: &'a FactorioContext) -> Self {
        Self { game }
    }
}


impl egui::Widget for UserContextEditor<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let response = ui.response();


        response
    }
}