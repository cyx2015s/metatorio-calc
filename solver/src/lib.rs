//! metatorio-solver：求解器内核（线性规划 + 组合枚举）。
//!
//! 本 crate 不依赖任何 UI 框架（egui/iced 等），
//! 问题描述（[`SolverData`]）与结果（[`SolverSolution`]）均为纯数据，
//! 可在无头环境独立测试与复用。

pub mod comb;
pub mod concept;
pub mod ruiz;
pub mod solver;

pub use comb::*;
pub use concept::*;
pub use ruiz::*;
pub use solver::*;
