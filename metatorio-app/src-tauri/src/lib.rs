//! Tauri adapter for the framework-independent `metatorio-runtime` layer.
//!
//! The frontend sends `AppMessage` values as JSON through the `dispatch`
//! command.  The runtime reducer returns a [`DispatchResult`]; side effects
//! (`RuntimeCommand`) are executed here — solving runs on a blocking worker
//! and its outcome is pushed to the frontend as a `solve-result` event.

use std::sync::Mutex;

use metatorio_data::store::PrototypeStore;
use metatorio_runtime::{
    document::AppDocument,
    message::{AppMessage, RuntimeCommand},
    solve::{Runtime, SolveResult},
    state::{DispatchResult, UiState},
};
use tauri::{AppHandle, Emitter, Manager, State};

/// Tauri-managed application runtime.
pub struct AppState {
    runtime: Mutex<Runtime>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            runtime: Mutex::new(Runtime::new()),
        }
    }
}

/// Minimal embedded game-data dump so the app can solve out of the box.
/// Replace with a real Factorio dump once data loading is wired to a
/// file dialog.
const DEMO_DUMP: &str = include_str!("../../../assets/data-raw-dump.json");

/// Load the embedded demo prototype store (idempotent).
#[tauri::command]
fn load_bundled_dump(state: State<'_, AppState>) -> Result<(), String> {
    let dump: serde_json::Value =
        serde_json::from_str(DEMO_DUMP).map_err(|error| error.to_string())?;
    let prototype =
        PrototypeStore::load(&dump).map_err(|error| format!("dump: {error}"))?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "runtime lock poisoned".to_string())?;
    runtime.install_prototype_store(prototype);
    Ok(())
}

/// Accept one user message and execute its side effects.
///
/// The dispatch plus any synchronous effects (persist) run on a blocking
/// worker; solving inside `Recompute` also happens there and its result is
/// emitted as a `solve-result` event.  The command only returns the
/// [`DispatchResult`] — the frontend refreshes its snapshot afterwards.
#[tauri::command]
async fn dispatch(app: AppHandle, message: AppMessage) -> Result<DispatchResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "runtime lock poisoned".to_string())?;
        let outcome = runtime.dispatch(message).map_err(|error| error.to_string())?;
        for command in &outcome.commands {
            execute_command(&app, &mut runtime, command);
        }
        Ok(outcome)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Current serializable document snapshot.
#[tauri::command]
fn get_document(state: State<'_, AppState>) -> Result<AppDocument, String> {
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| "runtime lock poisoned".to_string())?;
    Ok(runtime.state.document.clone())
}

/// Current transient UI selection snapshot.
#[tauri::command]
fn get_ui_state(state: State<'_, AppState>) -> Result<UiState, String> {
    let runtime = state
        .runtime
        .lock()
        .map_err(|_| "runtime lock poisoned".to_string())?;
    Ok(runtime.state.ui.clone())
}

/// Execute one side effect requested by the reducer.
///
/// Not every effect is wired yet: solving and pathful persistence are; the
/// rest (auto-plan, cleanup, updates, game-context loading from a dialog)
/// are logged until their frontend flows land.
fn execute_command(app: &AppHandle, runtime: &mut Runtime, command: &RuntimeCommand) {
    match command {
        RuntimeCommand::Recompute { project, factory } => match runtime.solve_factory(*project, *factory) {
            Ok(result) => emit_solve_result(app, result),
            Err(error) => emit_solve_error(app, error.to_string()),
        },
        RuntimeCommand::Persist { project, path } => {
            if let Some(path) = path {
                if let Err(error) = runtime.save_document_file(*project, path) {
                    eprintln!("persist failed: {error}");
                }
            }
            // Pathless persist (plain SaveProject) is a no-op until the app
            // tracks a per-project file path.
        }
        other => eprintln!("unhandled runtime command: {other:?}"),
    }
}

fn emit_solve_result(app: &AppHandle, result: SolveResult) {
    if let Err(error) = app.emit("solve-result", result) {
        eprintln!("failed to emit solve-result: {error}");
    }
}

fn emit_solve_error(app: &AppHandle, message: String) {
    if let Err(error) = app.emit("solve-error", message) {
        eprintln!("failed to emit solve-error: {error}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            load_bundled_dump,
            dispatch,
            get_document,
            get_ui_state
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
