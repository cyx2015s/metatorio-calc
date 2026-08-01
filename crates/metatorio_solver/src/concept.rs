//! 求解器内核的最小概念集：物品标识与流量集合。
//!
//! 从主 crate 的 `concept` 中拆出，仅保留求解器依赖的部分，
//! 使内核 crate 不依赖任何 UI 概念。

use std::{fmt::Debug, hash::Hash};

use indexmap::{IndexMap, IndexSet};

pub type AIndexMap<K, V> = IndexMap<K, V, ahash::RandomState>;
pub type AIndexSet<K> = IndexSet<K, ahash::RandomState>;

/// 物品标识（物品/流体/实体等），作为求解问题的"坐标"
pub type Flow<I> = AIndexMap<I, f64>;

pub trait ItemIdent: Debug + Clone + Eq + Hash + Send + Sync + 'static {}
impl<T> ItemIdent for T where T: Debug + Clone + Eq + Hash + Send + Sync + 'static {}
