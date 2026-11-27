//! metatorio-runtime：应用层状态机、事务历史与消息调度引擎。
//!
//! 本 crate 是连接上层宿主环境（Tauri GUI / Headless CLI）与底层核心库的枢纽，
//! 严格杜绝任何 GUI 框架依赖（不依赖 Tauri、Svelte 等），实现纯数据驱动的应用逻辑：
//! - **单向数据流架构**：用户操作与外壳事件统一封装为 [`AppMessage`]，经由 [`RuntimeState::dispatch`]
//!   调度执行，返回变更结果与副作用指令集合（[`CommandEffect`]）；
//! - **工程文档与版本管理**：定义强类型的可序列化工程对象模型（[`AppDocument`]、[`ProjectDocument`]、
//!   [`FactoryDocument`]、[`UniverseDocument`]），并支持平滑的数据版本迁移（[`migrate`]）；
//! - **撤销/重做事务历史**：细粒度追踪项目文档修改，保证任何编辑操作均可无损回退与前进；
//! - **求解与自动规划管线**：提供单工厂求解、宇宙工厂跨星球联合求解（[`universe`]）与全排列组合自动规划（[`auto_plan`]）；
//! - **结构化诊断**：产出带实体身份标识、便于机器消费与人类排错的高内聚 [`Diagnostic`] 结构。

pub mod auto_plan;
#[cfg(feature = "highs")]
pub mod column_gen;
pub mod diagnostic;
pub mod document;
pub mod id;
pub mod message;
pub mod migrate;
pub mod planet;
pub mod prototype;
pub mod solve;
pub mod state;
pub mod universe;
pub mod validate;

pub use diagnostic::Diagnostic;
pub use document::{
    AppDocument, AutoBeaconPlan, DOCUMENT_SCHEMA_VERSION, ExternalInput, FactoryDocument,
    FactorySettings, FlowTarget, InfiniteTechLevel, LocatedTarget, MechanicEntry, MechanicKind,
    Milestone, PlanningPreferences, ProjectDocument, ProjectSettings, RecipeProductivity,
    TargetExpression, TargetTerm, TimeScale, UniverseDocument, UniverseMechanicEntry,
};
pub use id::{
    ExternalInputId, FactoryId, MechanicId, ProjectId, TargetExpressionId, TargetId, TargetTermId,
    UniverseId,
};
pub use message::*;
pub use metatorio_solver::LpSolver;
pub use solve::{
    CommandEffect, ExpandedVarId, FlowBalance, MechanicSolution, ProductivityView, ProjectSnapshot,
    RecipeProductivityView, Runtime, SolveResult, SolveSnapshot, SolveStatus, ordered_milestones,
    parse_document_file, productivity_view, solve_snapshot, solve_snapshot_with,
    write_document_file,
};
pub use state::{DispatchResult, HistoryFocus, HistoryStatus, RuntimeError, RuntimeState};
pub use universe::{
    LocatedFlow, LocatedMechanicSolution, UniverseSolveResult, UniverseSolveStatus, solve_universe,
};
