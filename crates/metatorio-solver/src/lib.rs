//! metatorio-solver：求解器内核（线性规划 + 组合枚举）。
//!
//! 本 crate 不依赖任何 UI 框架（egui/iced 等），
//! 问题描述（[`SolverData`]）与结果（[`SolverSolution`]）均为纯数据，
//! 可在无头环境独立测试与复用。

pub mod comb;
pub mod concept;
pub mod control;
pub mod lp;
#[cfg(feature = "rust-lp")]
pub mod lp_experiments;
#[cfg(feature = "rust-lp")]
mod lp_hybrid;
pub mod lp_model;
#[cfg(feature = "rust-lp")]
mod lp_row_generation;
pub mod lp_solver;
#[cfg(feature = "rational-lp")]
pub use metatorio_lp as lp_rational;
pub mod solver;

pub use comb::*;
pub use concept::*;
pub use lp::*;
pub use lp_solver::LpSolver;
pub use solver::*;
