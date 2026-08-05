//! 原始变量（PrimVar）与展开结果。
//!
//! 命名来源：线性规划中配方是原始变量（Primal），物品守恒约束对应
//! 对偶变量（DualVar）。**1 个配置可展开为多个原始变量**（流体插值：
//! 温度区间两端各生成 1 个代数配方变量）。
//!
//! 插值端不显式存储：同一 config 的变量按展开顺序排列，**变量在
//! Expansion.variables 中的相对位置即温度组合端序号**——求解器以
//! `(config, position)` 定位变量，回代时按位置把各端解与流系数
//! 相乘求和即得实际产出/消耗（插值）。

use crate::dual_var::DualVar;

/// 带 ahash 的索引 Map（与 solver/data 同构）。
pub type AIndexMap<K, V> = indexmap::IndexMap<K, V, ahash::RandomState>;

/// 流：流标识 → 数量（负 = 消耗，正 = 产出）。
pub type Flow = AIndexMap<DualVar, f64>;

/// 配置的稳定标识：展开时按稳定键排序后分配的序号（与列表顺序无关，
/// 用户拖动配置列表不改变标识 → 求解结果稳定不重算）。
pub type ConfigId = usize;

/// 原始变量：1 个配置的 1 个代数变量。
///
/// 同一 config 的多个变量（流体插值端）在 `Expansion.variables` 中
/// **连续排列**，相对位置即端序号（由 TempFlow 的分裂顺序决定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrimVar {
    pub config: ConfigId,
}

/// 展开出的单个变量：变量 + 其流系数（1 单位运行量的消耗/产出）。
#[derive(Debug, Clone, PartialEq)]
pub struct ExpandedVariable {
    pub prim_var: PrimVar,
    pub flow: Flow,
}

/// 展开结果：全部代数变量（流体插值的多个端也在其中）。
///
/// 顺序与配置列表顺序无关（配置先按稳定键排序再编号展开）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Expansion {
    pub variables: Vec<ExpandedVariable>,
}

impl Expansion {
    pub fn len(&self) -> usize {
        self.variables.len()
    }

    pub fn is_empty(&self) -> bool {
        self.variables.is_empty()
    }
}
