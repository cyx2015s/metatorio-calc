//! metatorio-data：Factorio 原型数据 crate。
//!
//! 架构（两层）：
//! - **忠实层**（`generated_components` 插件）：与 dump 键 + 官方 schema
//!   （prototype-api.json）一一对应的组件结构体，字段名与游戏一致。
//! - **语义层**（后续 Phase 4 实现）：继承链匹配 → 领域角色（Crafter/Miner/...）
//!   归一化，供 metatorio_egui / metatorio_tauri / metatorio_iced 使用。
//!
//! 生成代码由 build.rs 在编译期从 `schema/prototype-api.json` 生成，
//! 生成器位于 `metatorio-data-codegen`。

/// 宽松反序列化：整数向 0 舍入、空 map 视为空 Vec（Lua→JSON 兼容）。
pub mod lenient;

/// 2.0 dump → 2.1 schema 数据适配器（就地规范化）。
pub mod adapt;

/// 预定义类型：schema 中需要自定义反序列化的类型（codegen 的 custom_type_map 引用）。
pub mod types;

/// Phase 3：原型仓库（按 (group, name) 索引的组件化原型记录）。
pub mod store;

/// Phase 4：扩展方法，默认值 getter、辅助
pub mod ext;

/// 生成代码的插件容器（避免与手写代码命名冲突）。
#[allow(clippy::all, dead_code, non_snake_case)]
pub(crate) mod generated_components {
    include!(concat!(env!("OUT_DIR"), "/generated.rs"));
}

pub use generated_components::*;
pub use types::{Color, EnergyAmount, MapPosition};
