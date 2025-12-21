use std::{collections::HashMap, fmt::Debug, hash::Hash};

pub(crate) trait RecipeLike {
    type KeyType;
    type ContextType;
    fn as_hash_map(&self, ctx: &Self::ContextType) -> HashMap<Self::KeyType, f64>;
}

pub(crate) trait ItemLike: Debug + Clone + Eq + Hash + PartialEq {}