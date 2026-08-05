//! 原始变量（PrimVar）与展开结果。
//!
//! 命名来源：线性规划中配方是原始变量（Primal），物品守恒约束对应
//! 对偶变量（DualVar）。**1 个配置可展开为多个原始变量**（流体插值：
//! 温度区间两端各生成 1 个代数配方变量），求解后按变量比例回代插值
//! 实际输入温度。

use crate::dual_var::DualVar;

/// 带 ahash 的索引 Map（与 solver/data 同构）。
pub type AIndexMap<K, V> = indexmap::IndexMap<K, V, ahash::RandomState>;

/// 流：流标识 → 数量（负 = 消耗，正 = 产出）。
pub type Flow = AIndexMap<DualVar, f64>;

/// 插值端（流体温度区间 → 2^k 组合的其中一端）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Variant {
    /// 非插值配置（单变量）。
    Single,
    /// 流体插值组合：位掩码，第 i 位 = 第 i 个流体输入取高温度端。
    /// 第一版单流体输入：0 = 最低温度端，1 = 最高温度端。
    Interp(u8),
}

/// 配置的稳定标识：展开时按稳定键排序后分配的序号（与列表顺序无关，
/// 用户拖动配置列表不改变标识 → 求解结果稳定不重算）。
pub type ConfigId = usize;

/// 原始变量：1 个配置（ConfigId）的 1 个代数变量（插值端）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrimVar {
    pub config: ConfigId,
    pub variant: Variant,
}

/// 展开出的单个变量：变量 + 其流系数（1 单位运行量的消耗/产出）。
#[derive(Debug, Clone, PartialEq)]
pub struct ExpandedVariable {
    pub prim_var: PrimVar,
    pub flow: Flow,
}

/// 展开结果：全部代数变量（流体插值的 2 端也在其中）。
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
