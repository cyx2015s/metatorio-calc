use std::{collections::HashMap, fmt::Debug, hash::Hash};

use crate::SubView;

pub trait RecipeLike {
    type KeyType: Debug + Clone + Eq + Hash + PartialEq;
    type ContextType;
    fn as_hash_map(&self, ctx: &Self::ContextType) -> HashMap<Self::KeyType, f64>;
}

pub trait ItemLike: Debug + Clone + Eq + Hash + PartialEq {}

pub trait GameContextCreatorView : SubView {
    /// 如何有可能，尝试创建新的子视图，将所有权转移出去
    fn try_create_subview(&mut self) -> Option<Box<dyn SubView>>;
}
