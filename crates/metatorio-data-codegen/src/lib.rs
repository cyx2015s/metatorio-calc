//! metatorio-data-codegen：从 Factorio prototype-api.json 生成组件化原型结构体。
//!
//! 供 `metatorio-data` 的 build.rs 在编译期调用；也可作为独立工具/测试使用。

pub mod config;
pub mod emit;
pub mod schema;
pub mod type_map;

pub use config::Config;
pub use emit::{GenStats, generate};
pub use schema::Schema;
