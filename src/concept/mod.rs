use std::{collections::HashMap, fmt::Debug, hash::Hash};

use crate::SubView;

pub trait RecipeLike: Send {
    type KeyType: ItemLike;
    type ContextType;
    fn as_hash_map(&self, ctx: &Self::ContextType) -> HashMap<Self::KeyType, f64>;
}

pub trait ItemLike: Debug + Clone + Eq + Hash + PartialEq {}

pub trait GameContextCreatorView : SubView {
    /// 如何有可能，尝试创建新的子视图，将所有权转移出去
    fn set_subview_sender(&mut self, sender: std::sync::mpsc::Sender<Box<dyn SubView>>);
}
