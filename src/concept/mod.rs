use std::{collections::HashMap, fmt::Debug, hash::Hash};

use crate::Subview;

pub trait RecipeLike: Send {
    type ItemType: ItemLike;
    type ContextType;
    fn as_hash_map(&self, ctx: &Self::ContextType) -> HashMap<Self::ItemType, f64>;
}

pub trait ItemLike: Debug + Clone + Eq + Hash + PartialEq {}

pub trait GameContextCreatorView: Subview {
    fn set_subview_sender(&mut self, sender: std::sync::mpsc::Sender<Box<dyn Subview>>);
}

pub trait RecipeLikeEditorView: RecipeLike {
    fn editor_view(&mut self, ui: &mut egui::Ui, ctx: &Self::ContextType);
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
