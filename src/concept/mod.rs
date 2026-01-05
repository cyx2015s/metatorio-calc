use std::{collections::HashMap, fmt::Debug, hash::Hash};

use crate::Subview;

pub trait AsFlow: Send {
    type ItemIdentType: ItemIdent;
    type ContextType;
    fn as_flow(&self, ctx: &Self::ContextType) -> HashMap<Self::ItemIdentType, f64>;
}

pub trait ItemIdent: Debug + Clone + Eq + Hash + PartialEq {}

pub trait GameContextCreatorView: Subview {
    fn set_subview_sender(&mut self, sender: std::sync::mpsc::Sender<Box<dyn Subview>>);
}

pub trait AsFlowEditor: AsFlow {
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &Self::ContextType);
    /// 传入的为系数，和 as_hash_map 返回值顺序一致
    /// 默认实现是不处理，不知道暂时也没有关系，但是和展示界面会有关
    fn notify_solution(&mut self, solution: Vec<f64>) {
        println!("RecipeLike::notify_solution called with {:?}", solution);
    }
}

pub trait AsFlowSource: Subview {
    type RecipeType: AsFlowEditor;
    /// 传递创建的配方信息
    fn set_recipe_like_sender(
        &mut self,
        sender: std::sync::mpsc::Sender<
            Box<
                dyn AsFlowEditor<
                        ItemIdentType = <Self::RecipeType as AsFlow>::ItemIdentType,
                        ContextType = <Self::RecipeType as AsFlow>::ContextType,
                    >,
            >,
        >,
    );
}
