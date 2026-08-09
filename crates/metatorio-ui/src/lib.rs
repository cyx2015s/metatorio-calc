//! New metatorio UI shell.
//!
//! The UI is deliberately split into three layers:
//! - [`state`] owns serializable factory data and transient presentation state.
//! - [`message`] describes user intent without depending on egui.
//! - [`view`] renders state and emits messages; it never mutates the backend state.

pub mod message;
pub mod state;
pub mod view;

pub use message::{AppMessage, Command, MechanicId, MechanicKind, MechanicMessage};
pub use state::{AppState, FactoryState, MechanicEntry, UiState};
