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
use crate::prim_var::{ConfigId, ExpandedVariable, Flow, PrimVar};
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
    /// 变温流体数（单点温度不计入；副本数 = 2^k，k ≤ 8）。
    fluid_count: usize,
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
            fluid_count: 0,
        }
    }

    /// 固定流（物品/能量等）：作用到所有副本。`amount` 负数 = 消耗。
    pub fn add(&mut self, var: DualVar, amount: f64) {
        for copy in &mut self.copies {
            add_flow(copy, var.clone(), amount);
        }
    }

    /// 变温流体：每个现有副本分裂为 lo 端 / hi 端两个副本。
    ///
    /// `lo`/`hi` 不排序（端点语义，表达温度相关性）；`lo == hi` 退化为固定流。
    /// 同时添加流体本体与配套的虚拟热量流（`amount × (T − default) × capacity`）。
    pub fn add_fluid(&mut self, ctx: &Context, name: &str, amount: f64, lo: f64, hi: f64) {
        let add_single = |flow: &mut Flow, temp: f64| {
            add_flow(flow, DualVar::Fluid { name: name.into() }, amount);
            add_flow(
                flow,
                DualVar::FluidHeat {
                    filter: name.into(),
                },
                fluid_heat(ctx, name, amount, temp),
            );
        };
        if lo == hi {
            // 单点温度：不分裂
            for copy in &mut self.copies {
                add_single(copy, lo);
            }
            return;
        }
        let mut low: Flow = Default::default();
        let mut high: Flow = Default::default();
        add_single(&mut low, lo);
        add_single(&mut high, hi);
        self.add_dual(&low, &high);
    }

    /// 双端流：每个现有副本分裂为 lo 端 / hi 端两个副本，分别合并 `lo`/`hi` 的系数。
    ///
    /// 这是表达**相关性**的原语：任意系数（燃料消耗、发电量、其他物品……）都可随
    /// 端变化——调用方构造两个完整 Flow（如低温端/高温端）传入；`add_fluid` 是它的
    /// 便捷包装（只有热量随端变化）。分裂次数 = 端序号位数（k ≤ 8）。
    ///
    /// 典型场景：锅炉输入不同温度的水 → 加热到目标温度所需的燃料量不同；
    /// 汽轮机输入不同温度的蒸汽 → 发电量不同。两个 Flow 的差异就是相关性。
    pub fn add_dual(&mut self, lo: &Flow, hi: &Flow) {
        assert!(
            self.fluid_count < 8,
            "分裂次数超过 8（2^k 组合爆炸），当前实现不支持"
        );
        let mut next = Vec::with_capacity(self.copies.len() * 2);
        for copy in &self.copies {
            let mut low = copy.clone();
            merge_flow(&mut low, lo);
            let mut high = copy.clone();
            merge_flow(&mut high, hi);
            next.push(low);
            next.push(high);
        }
        self.copies = next;
        self.fluid_count += 1;
    }

    /// 收尾：副本 → 展开变量。变量顺序 = 副本顺序（同一 config 连续排列，
    /// 相对位置即温度组合端序号，由求解器以 (config, position) 定位）。
    pub fn into_variables(self, config: ConfigId) -> Vec<ExpandedVariable> {
        self.copies
            .into_iter()
            .map(|flow| ExpandedVariable {
                prim_var: PrimVar { config },
                flow,
            })
            .collect()
    }
}
