//! 展开中的临时配方：内部隐式持有多个副本（变温流体组合端）。
//!
//! - `add`：固定流（物品/能量等），作用到所有副本
//! - `add_fluid`：变温流体，**每个现有副本自动分裂为 2 个**（lo 端 + hi 端），
//!   副本总数 = 2^k（k = 变温流体数）
//! - 温度端点**不排序**（`lo > hi` 合法）：副本索引的二进制位 = 各流体的端点选择
//!   （第 j 次 `add_fluid` 对应第 j 位），同一 PrimVar 的 `Interp(mask)` 同时决定
//!   所有流体的端点——**表达流体温度相关性**的能力由此保留
//! - 单点温度（`lo == hi`）退化为固定流，不分裂

use crate::context::Context;
use crate::dual_var::DualVar;
use crate::prim_var::{ExpandedVariable, Flow, PrimVar};
use metatorio_data::generated_components::FluidComponent;
use metatorio_data::store::PrototypeGroup;

fn add_flow(flow: &mut Flow, var: DualVar, amount: f64) {
    if amount == 0.0 {
        return;
    }
    *flow.entry(var).or_insert(0.0) += amount;
}

/// 合并另一份流系数（0 系数跳过，保持稀疏）。
fn merge_flow(flow: &mut Flow, other: &Flow) {
    for (var, amount) in other {
        if *amount != 0.0 {
            *flow.entry(var.clone()).or_insert(0.0) += amount;
        }
    }
}

/// 变温流体热量：amount × (温度 − 默认温度) × 比热容（焦耳）。
fn fluid_heat(ctx: &Context, name: &str, amount: f64, temperature: f64) -> f64 {
    let Some(record) = ctx.prototype.get(PrototypeGroup::Fluid, name) else {
        return 0.0;
    };
    let Some(fluid) = record.component::<FluidComponent>() else {
        return 0.0;
    };
    let capacity = fluid.heat_capacity().amount;
    let default = fluid.default_temperature;
    amount * (temperature - default) * capacity
}

/// 展开中的临时配方。
#[derive(Debug, Clone, PartialEq)]
pub struct TempFlow {
    /// 每个副本的流系数；副本索引低 k 位 = 各变温流体的端点选择。
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

    /// 变温流体：温度区间 [lo, hi] 按可用温度表**一分为 N**（单态决策）。
    ///
    /// 查询 `PrototypeStore::fluid_temperatures()`，取区间内可用温度逐个生成决策
    /// （每个决策一个单点温度键，不混合不插值）；区间为空 → 退化为单点（默认温度）。
    pub fn add_fluid(&mut self, ctx: &Context, name: &str, amount: f64, lo: f64, hi: f64) {
        let mut temps: Vec<i32> = ctx
            .prototype
            .fluid_temperatures()
            .get(name)
            .map(|ts| {
                ts.iter()
                    .filter(|&&t| f64::from(t) >= lo && f64::from(t) <= hi)
                    .copied()
                    .collect()
            })
            .unwrap_or_default();
        // 无可用温度（或单点区间）：退化为单点（默认温度）
        if temps.is_empty() {
            let default = ctx
                .prototype
                .get(PrototypeGroup::Fluid, name)
                .and_then(|r| r.component::<FluidComponent>())
                .map(|f| f.default_temperature as i32)
                .unwrap_or(0);
            temps.push(default);
        }
        self.add_fluid_multi(ctx, name, amount, &temps);
    }

    /// 多温度决策：每个现有副本 × N 分裂（N = 可用温度数）。
    ///
    /// 每个温度生成 `Fluid{name@T}` 单点键 + 携带热量（`amount × (T − default) × capacity`）；
    /// 分裂顺序 = temps 顺序（稳定，调用方负责排序）。
    pub fn add_fluid_multi(&mut self, ctx: &Context, name: &str, amount: f64, temps: &[i32]) {
        let add_single = |flow: &mut Flow, temp: i32| {
            add_flow(
                flow,
                DualVar::Fluid {
                    name: name.into(),
                    temperature: [temp; 2],
                },
                amount,
            );
        };
        let flows = temps.iter().map(|&temp| {
            let mut flow = Flow::default();
            add_single(&mut flow, temp);
            flow
        });
        self.add_parallel(flows);
    }

    pub fn add_parallel(&mut self, others: impl IntoIterator<Item = Flow>) {
        let mut next = Vec::with_capacity(self.copies.len());
        for other in others {
            for copy in &self.copies {
                let mut variant = copy.clone();
                merge_flow(&mut variant, &other);
                next.push(variant);
            }
        }
        self.copies = next;
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
            })
            .collect()
    }
}
