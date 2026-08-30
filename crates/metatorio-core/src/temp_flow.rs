//! 展开中的临时配方：内部隐式持有多个副本（变温流体组合端）。
//!
//! - `add`：固定流（物品/能量/单个温度区间），作用到所有副本
//! - `add_parallel`：由调用方显式提供每个候选副本，保留候选之间的相关性
//! - 流体温度仍由 `DualVar::Fluid::temperature` 的区间表示；区间之间的
//!   转换流由求解器/调用方添加，不在这里查询原型温度表或自动展开

use crate::dual_var::DualVar;
use crate::prim_var::{ExpandedVariable, Flow, PrimVar};

fn add_flow(flow: &mut Flow, var: DualVar, amount: f64) {
    if amount == 0.0 {
        return;
    }
    let new_amount = flow.get(&var).copied().unwrap_or_default() + amount;
    if new_amount == 0.0 {
        flow.shift_remove(&var);
    } else {
        flow.insert(var, new_amount);
    }
}

/// 合并另一份流系数（0 系数跳过，保持稀疏）。
fn merge_flow(flow: &mut Flow, other: &Flow) {
    for (var, amount) in other {
        add_flow(flow, var.clone(), *amount);
    }
}

/// 展开中的临时配方。
#[derive(Debug, Clone, PartialEq)]
pub struct TempFlow {
    /// 每个由调用方显式组合出的流系数副本。
    copies: Vec<Flow>,
}

impl Default for TempFlow {
    fn default() -> Self {
        Self::new()
    }
}

impl TempFlow {
    /// 空配方：1 个空副本。
    pub fn new() -> Self {
        Self {
            copies: vec![Flow::default()],
        }
    }

    /// 固定流（物品/能量等）：作用到所有副本。`amount` 负数 = 消耗。
    pub fn add(&mut self, var: DualVar, amount: f64) {
        for copy in &mut self.copies {
            add_flow(copy, var.clone(), amount);
        }
    }

    /// 将每个现有副本与所有候选流组合。候选维度放在低位，
    /// 因而后加入的流体改变较低的变量位置。
    pub fn add_parallel(&mut self, others: impl IntoIterator<Item = Flow>) {
        let others = others.into_iter().collect::<Vec<_>>();
        let mut next = Vec::with_capacity(self.copies.len() * others.len());
        for copy in &self.copies {
            for other in &others {
                let mut variant = copy.clone();
                merge_flow(&mut variant, other);
                next.push(variant);
            }
        }
        self.copies = next;
    }

    /// Scale every flow in every manually supplied variant.
    pub fn scale(&mut self, factor: f64) {
        if factor == 1.0 {
            return;
        }
        for copy in &mut self.copies {
            for amount in copy.values_mut() {
                *amount *= factor;
            }
        }
    }

    /// 收尾：副本 → 展开变量。变量顺序 = 副本顺序（同一 config 连续排列，
    /// 相对位置即温度组合端序号，由求解器以 (config, position) 定位）。
    pub fn into_variables<C: Clone>(self, config: C) -> Vec<ExpandedVariable<C>> {
        self.copies
            .into_iter()
            .map(|flow| ExpandedVariable {
                prim_var: PrimVar {
                    inner: config.clone(),
                },
                flow,
                cost: 0.0,
            })
            .collect()
    }
}
