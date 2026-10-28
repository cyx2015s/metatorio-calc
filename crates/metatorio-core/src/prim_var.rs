//! Each valid mechanism expands to one algebraic variable, in caller order.

use crate::dual_var::DualVar;

/// 带 ahash 的索引 Map（与 solver/data 同构）。
pub type AIndexMap<K, V> = indexmap::IndexMap<K, V, ahash::RandomState>;

/// 流：流标识 → 数量（负 = 消耗，正 = 产出）。
pub type Flow = AIndexMap<DualVar, f64>;

/// 展开出的单个变量：变量 + 其流系数（1 单位运行量的消耗/产出）。
#[derive(Debug, Clone, PartialEq)]
pub struct ExpandedVariable<C> {
    pub config: C,
    pub flow: Flow,
    /// 该变量的单位成本（LP 目标系数）。不同物品转换流可有不同代价
    /// （如太阳能随表面倍率变化的面积成本）；由展开函数按机制计算写入。
    pub cost: f64,
}

/// 展开结果：全部机制的代数变量。
///
/// 顺序由调用方传入的配置顺序决定（稳定性由调用方保证）。
#[derive(Debug, Clone, PartialEq)]
pub struct Expansion<C> {
    pub variables: Vec<ExpandedVariable<C>>,
}

impl<C> Default for Expansion<C> {
    fn default() -> Self {
        Self {
            variables: Vec::new(),
        }
    }
}

impl<C> Expansion<C> {
    pub fn len(&self) -> usize {
        self.variables.len()
    }

    pub fn is_empty(&self) -> bool {
        self.variables.is_empty()
    }
}
