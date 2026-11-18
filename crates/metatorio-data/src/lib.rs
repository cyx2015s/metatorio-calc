#![feature(mut_restriction)]

//! metatorio-data：Factorio 游戏原型（Prototype）数据与模型仓储层。
//!
//! 本 crate 负责高效、准确地反序列化、存储并索引异星工厂的原型 dump 数据：
//! - **代码生成层（Codegen）**：由 `metatorio-data-codegen` 基于游戏官方 schema (`prototype-api.json`)
//!   在编译期自动生成全部组件结构体，保证字段类型与游戏数据结构强一致；
//! - **宽松容错层（[`lenient`] / [`adapt`]）**：针对 Lua 导出 JSON 时的特性（如空 table 在 JSON 中为 `[]`
//!   或 `{}` 的歧义、数值舍入模式、历史 dump 版本差异）提供就地规范化适配；
//! - **原型仓储系统（[`store`]）**：提供基于组件化模式的 [`PrototypeStore`](store::PrototypeStore)，
//!   按原型分类与原型名高效检索实体、配方、物品、流体、品质与科技属性；
//! - **领域衍生规则**：包含基于制造配方拓扑的物品质量推导（[`item_weight`]）、聚变与能量学计算（[`fusion`]）等。

/// 宽松反序列化：整数向 0 舍入、空 map 视为空 Vec（Lua→JSON 兼容）。
pub mod lenient;

/// 历史 dump → 当前 schema 数据适配器（就地规范化）。
pub mod adapt;

/// 预定义类型：schema 中需要自定义反序列化的类型（codegen 的 custom_type_map 引用）。
pub mod types;

/// 原型仓库：按 (group, name) 索引的组件化原型记录，支撑整个上层的静态数据查询。
pub mod store;

pub mod crafting;
pub mod fusion;
/// 基于配方推导的物品重量及计算依据追踪。
pub mod item_weight;

/// 原型组件扩展方法、默认值访问与便捷属性 getter。
pub mod ext;

/// 生成代码的插件容器（避免与手写代码命名冲突）。
#[allow(clippy::all, dead_code, non_snake_case)]
pub(crate) mod generated_components {
    include!(concat!(env!("OUT_DIR"), "/generated.rs"));
}

pub use generated_components::*;
pub use types::{Color, EnergyAmount, MapPosition};
