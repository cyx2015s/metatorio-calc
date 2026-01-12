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

pub type Flow<I> = HashMap<I, f64>;

pub trait AsFlow: Send + ContextBound {
    /// 传递物品流信息
    fn as_flow(&self, ctx: &Self::ContextType) -> Flow<Self::ItemIdentType>;
    /// 执行成本，默认返回 1.0
    fn cost(&self, _ctx: &Self::ContextType) -> f64 {
        1.0
    }
}

pub type AsFlowSender<I, C> =
    std::sync::mpsc::Sender<Box<dyn AsFlowEditor<ItemIdentType = I, ContextType = C>>>;

pub trait ItemIdent: Debug + Clone + Eq + Hash {}
pub trait GameContextCreatorView: Subview {
    fn set_subview_sender(&mut self, sender: std::sync::mpsc::Sender<Box<dyn Subview>>);
}

pub trait AsFlowEditor: AsFlow + EditorView {}

impl<T> AsFlowEditor for T where T: AsFlow + EditorView {}

pub trait AsFlowEditorSource: EditorView + ContextBound {
    /// 传递创建的配方信息
    fn set_as_flow_sender(&mut self, sender: AsFlowSender<Self::ItemIdentType, Self::ContextType>);

    /// TODO
    /// 游戏机制提供器可选：自动填充逻辑
    fn auto_populate(
        &mut self,
        _ctx: &Self::ContextType,
        _flows: &HashMap<usize, Flow<Self::ItemIdentType>>,
    ) -> Vec<
        Box<dyn AsFlowEditor<ItemIdentType = Self::ItemIdentType, ContextType = Self::ContextType>>,
    > {
        // 默认不实现任何自动填充逻辑
        vec![]
    }

    /// 在规划界面点击物品时，可以提供一些推荐配方
    fn hint_populate(
        &mut self,
        _ctx: &Self::ContextType,
        _flows: &HashMap<usize, Flow<Self::ItemIdentType>>,
        _item: &Self::ItemIdentType,
        _value: f64,
    ) -> Vec<
        Box<dyn AsFlowEditor<ItemIdentType = Self::ItemIdentType, ContextType = Self::ContextType>>,
    > {
        vec![]
    }
}
