//! metatorio-data：Factorio 原型数据 crate。
//!
//! 架构（两层）：
//! - **忠实层**（`generated_components` 模块）：与 dump 键 + 官方 schema
//!   （prototype-api.json）一一对应的组件结构体，字段名与游戏一致。
//! - **语义层**（后续 Phase 4 实现）：继承链匹配 → 领域角色（Crafter/Miner/...）
//!   归一化，供 metatorio_egui / metatorio_tauri / metatorio_iced 使用。
//!
//! 生成代码由 build.rs 在编译期从 `schema/prototype-api.json` 生成，
//! 生成器位于 `metatorio-data-codegen`。

/// 生成代码的模块容器（避免与手写代码命名冲突）。
pub mod generated_components {
    include!(concat!(env!("OUT_DIR"), "/generated.rs"));
}

pub use generated_components::*;

/// 能量值（游戏内为字符串，如 "5MJ"、"300kW"、"1.2MW"）。
///
/// 保留原始字符串，解析为数值由消费方按需进行（或后续 Phase 在此补充解析器）。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EnergyAmount(pub String);
