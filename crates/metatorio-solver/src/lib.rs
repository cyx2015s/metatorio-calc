//! metatorio-solver：线性规划（LP）模型构建与多后端求解引擎。
//!
//! 本 crate 负责将 Factorio 工厂生产网络转化为规范的线性规划问题，并调度后端求解器：
//! - **多求解后端抽象（[`LpSolver`]）**：
//!   - `HiGHS`：工业级高性能 LP 求解器（新项目默认推荐）；
//!   - `SoPlex`：支持严格有理数运算的高精度单纯形求解器（借助本地 Boost 补丁消除数值误差）；
//!   - `Clarabel`：现代化内点法（Interior Point Method）二次锥规划求解器；
//!   - `microlp`：纯 Rust 轻量级单纯形求解器；
//!   - `rust-certified`（`metatorio-lp`）：纯 Rust 精确重构与基验证引擎；
//! - **数据模型解耦**：
//!   - 求解输入（[`SolverData`]）与求解产物（[`SolverSolution`]）为纯数据结构，
//!     与前端渲染及外壳环境完全解耦，可在无头环境独立测试；
//! - **组合搜索与行列生成**：提供模块化的模型裁剪、变量生成与增量求解基础设施。

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
#[cfg(feature = "soplex")]
mod soplex_backend;
#[cfg(feature = "rational-lp")]
pub use metatorio_lp as lp_rational;
pub mod solver;

pub use comb::*;
pub use concept::*;
pub use lp::*;
pub use lp_solver::LpSolver;
pub use solver::*;
