//! Event payload contracts shared by producers and desktop binding generation.
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

/// Legacy string errors and structured domain diagnostics share an event channel.
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
