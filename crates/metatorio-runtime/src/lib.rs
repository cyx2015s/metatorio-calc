//! Application-layer state and messages for the Tauri frontend.
//!
//! This crate deliberately does not depend on Tauri, egui, or Svelte.  It
//! describes user intent and the serializable project document.  A frontend
//! turns input events into [`AppMessage`] values; an outer runtime applies
//! them and schedules persistence or solving as needed.

pub mod auto_plan;
pub mod document;
pub mod id;
pub mod message;
pub mod migrate;
pub mod planet;
pub mod prototype;
pub mod solve;
pub mod state;

pub use document::{
    AppDocument, AutoBeaconPlan, DOCUMENT_SCHEMA_VERSION, ExternalInput, FactoryDocument,
    FactorySettings, FlowTarget, InfiniteTechLevel, MechanicEntry, MechanicKind, Milestone,
    PlanningPreferences, ProjectDocument, ProjectSettings, RecipeProductivity, TargetExpression,
    TargetTerm, TimeScale,
};
pub use id::{
    ExternalInputId, FactoryId, MechanicId, ProjectId, TargetExpressionId, TargetId, TargetTermId,
};
pub use message::*;
pub use solve::{
    CommandEffect, ExpandedVarId, FlowBalance, MechanicSolution, ProductivityView,
    RecipeProductivityView, Runtime, SolveResult, SolveStatus,
};
pub use state::{DispatchResult, RuntimeError, RuntimeState, UiState};
