//! 原始变量（PrimVar）与展开结果。
//!
//! 命名来源：线性规划中配方是原始变量（Primal），物品守恒约束对应
//! 对偶变量（DualVar）。**1 个配置可展开为多个原始变量**（流体插值：
//! 温度区间两端各生成 1 个代数配方变量）。
//!
//! `PrimVar<C>` 泛型于 config 标识类型：调用方自选任意 `Hash + Eq` 类型
//! （如配置 ID、`AIndexMap` 的键），展开不做编号分配——同一配置的多个
//! 变量（流体插值端）在 `Expansion.variables` 中**连续排列**，相对位置即
//! 端序号（由 TempFlow 的分裂顺序决定）；求解器以 `(config, position)`
//! 定位变量，回代时按位置把各端解与流系数相乘求和即得实际产出/消耗。

use crate::dual_var::DualVar;

/// 带 ahash 的索引 Map（与 solver/data 同构）。
pub type AIndexMap<K, V> = indexmap::IndexMap<K, V, ahash::RandomState>;

/// 流：流标识 → 数量（负 = 消耗，正 = 产出）。
pub type Flow = AIndexMap<DualVar, f64>;

/// 原始变量：1 个配置的 1 个代数变量。
///
/// `C`：配置标识（调用方自选，`Hash + Eq`；如配置 ID 或 `AIndexMap` 的键）。
/// 同一 config 的多个变量（流体插值端）在 `Expansion.variables` 中**连续排列**，
/// 相对位置即端序号（由 TempFlow 的分裂顺序决定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrimVar<C> {
    pub inner: C,
}

/// 展开出的单个变量：变量 + 其流系数（1 单位运行量的消耗/产出）。
#[derive(Debug, Clone, PartialEq)]
pub struct ExpandedVariable<C> {
    pub prim_var: PrimVar<C>,
    pub flow: Flow,
}

/// 展开结果：全部代数变量（流体插值的多个端也在其中）。
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
