use std::{collections::HashMap, fmt::Debug, hash::Hash};


pub trait Subview: Send {
    fn view(&mut self, ui: &mut egui::Ui);
    fn should_close(&self) -> bool {
        false
    }
}

pub trait ContextBound {
    type ContextType;
    type ItemIdentType: ItemIdent;
}

pub trait EditorView: Send + ContextBound {
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &Self::ContextType);
}

pub trait AsFlow: Send + ContextBound {
    fn as_flow(&self, ctx: &Self::ContextType) -> HashMap<Self::ItemIdentType, f64>;
}

pub type AsFlowSender<I, C> = std::sync::mpsc::Sender<
    Box<
        dyn AsFlowEditor<
                ItemIdentType = I,
                ContextType = C,
            >,
    >,
>;

pub trait ItemIdent: Debug + Clone + Eq + Hash {}

pub trait GameContextCreatorView: Subview {
    fn set_subview_sender(&mut self, sender: std::sync::mpsc::Sender<Box<dyn Subview>>);
}

pub trait AsFlowEditor: AsFlow + EditorView {
    fn notify_solution(&mut self, solution: f64) {
        println!("AsFlowEditor::notify_solution called with {:?}", solution);
    }
}

pub trait AsFlowSource: EditorView + ContextBound {
    /// 传递创建的配方信息
    fn set_as_flow_sender(&mut self, sender: AsFlowSender<Self::ItemIdentType, Self::ContextType>);
}
