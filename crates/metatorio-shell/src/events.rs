//! 生产端与桌面端绑定生成共用的事件载荷契约。
use metatorio_runtime::{Diagnostic, FactoryId, ProjectId, UniverseId, solve::AutoPlanProgress};
use serde::Serialize;

#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
pub struct AutoPlanProgressEvent {
    pub project: ProjectId,
    pub factory: FactoryId,
    pub universe: Option<UniverseId>,
    pub run_id: u64,
    pub running: bool,
    #[serde(flatten)]
    pub progress: AutoPlanProgress,
}

#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
pub struct FactorySolveError {
    pub project: ProjectId,
    pub factory: FactoryId,
    pub revision: u64,
    #[serde(flatten)]
    pub diagnostic: Diagnostic,
}

#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
pub struct UniverseSolveError {
    pub project: ProjectId,
    pub universe: UniverseId,
    pub revision: u64,
    pub message: String,
}

/// 传统字符串错误与结构化领域诊断共用同一个事件通道。
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum EventError {
    Message(String),
    Diagnostic(Diagnostic),
}

#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum SolveErrorPayload {
    Message(String),
    Failure(FactorySolveError),
}
