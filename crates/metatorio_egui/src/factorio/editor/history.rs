use std::fmt::Debug;

use crate::factorio::planner::ProjectInstance;
pub struct UndoRedoCommand {
    pub description: String,
    pub undo: Box<dyn FnMut(&mut ProjectInstance)>,
    pub redo: Box<dyn FnMut(&mut ProjectInstance)>,
}

impl Debug for UndoRedoCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UndoRedoCommand")
            .field("description", &self.description)
            .finish()
    }
}

#[derive(Debug)]
pub struct UndoRedoStack {
    pub undo_stack: Vec<UndoRedoCommand>,
    pub redo_stack: Vec<UndoRedoCommand>,
}
