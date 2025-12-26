use std::{collections::HashMap, fmt::Debug, hash::Hash};

use crate::Subview;

pub trait RecipeLike: Send {
    type ItemType: ItemLike;
    type ContextType;
    fn as_hash_map(&self, ctx: &Self::ContextType) -> Vec<HashMap<Self::ItemType, f64>>;
}

pub trait ItemLike: Debug + Clone + Eq + Hash + PartialEq {}

pub trait GameContextCreatorView: Subview {
    fn set_subview_sender(&mut self, sender: std::sync::mpsc::Sender<Box<dyn Subview>>);
}

pub trait RecipeLikeEditorView: RecipeLike {
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &Self::ContextType);
    /// 传入的为系数，和 as_hash_map 返回值顺序一致
    /// 默认实现是不处理，不知道暂时也没有关系，但是和展示界面会有关
    fn notify_solution(&self, solution: Vec<f64>) {
        println!("RecipeLike::notify_solution called with {:?}", solution);
    }
}

pub trait RecipeLikeCreatorView: Subview {
    type RecipeType: RecipeLikeEditorView;
    /// 传递创建的配方信息
    fn set_recipe_like_sender(
        &mut self,
        sender: std::sync::mpsc::Sender<
            Box<
                dyn RecipeLikeEditorView<
                        ItemType = <Self::RecipeType as RecipeLike>::ItemType,
                        ContextType = <Self::RecipeType as RecipeLike>::ContextType,
                    >,
            >,
        >,
    );
}

pub trait FactoryView: Subview {
    type ItemType: ItemLike;
    type ContextType;
}