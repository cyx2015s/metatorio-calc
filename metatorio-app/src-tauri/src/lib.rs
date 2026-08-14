//! Tauri adapter for the framework-independent `metatorio-runtime` layer.
//!
//! The frontend sends `AppMessage` values as JSON through the `dispatch`
//! command.  The runtime reducer returns a [`DispatchResult`]; side effects
//! (`RuntimeCommand`) are executed here — solving runs on a blocking worker
//! and its outcome is pushed to the frontend as a `solve-result` event.
//!
//! This layer also owns the game-data context (loaded prototype store + icon
//! directory) and project file persistence, both of which are inherently
//! app-side concerns.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

use metatorio_data::store::{PrototypeGroup, PrototypeRecord, PrototypeStore};
use metatorio_data::{
    CraftingMachineComponent, ItemSubGroupComponent, PrototypeBaseComponent,
};
use metatorio_runtime::{
    document::AppDocument,
    id::ProjectId,
    message::{AppMessage, RuntimeCommand},
    solve::Runtime,
    state::{DispatchResult, UiState},
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

/// Minimal embedded game-data dump so the app can solve out of the box.
/// Replace with a real Factorio dump once data loading is wired to a
/// file dialog.
const DEMO_DUMP: &str = include_str!("../dumps/demo_dump.json");

// ── Managed state ─────────────────────────────────────────────────

pub struct AppState {
    runtime: Mutex<Runtime>,
    context: Mutex<Option<GameContext>>,
    project_paths: Mutex<HashMap<ProjectId, String>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            runtime: Mutex::new(Runtime::new()),
            context: Mutex::new(None),
            project_paths: Mutex::new(HashMap::new()),
        }
    }
}

/// Loaded game context: the prototype store the solver and the selector use,
/// plus the directory of game icon PNGs produced by `--dump-icon-sprites`.
struct GameContext {
    prototype: PrototypeStore,
    icon_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextInfo {
    pub loaded: bool,
    pub groups: Vec<GroupCount>,
    pub icon_root: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupCount {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogEntry {
    pub name: String,
    pub group: String,
    pub subgroup: String,
    pub icon_type: String,
    pub module_slots: Option<u16>,
}

// ── Context loading ───────────────────────────────────────────────

fn context_info(state: &AppState) -> ContextInfo {
    let context = state.context.lock().ok();
    let Some(Some(context)) = context.as_deref() else {
        return ContextInfo {
            loaded: false,
            groups: Vec::new(),
            icon_root: None,
        };
    };
    ContextInfo {
        loaded: true,
        groups: context
            .prototype
            .groups
            .iter()
            .map(|(group, records)| GroupCount {
                name: format!("{group:?}"),
                count: records.len(),
            })
            .collect(),
        icon_root: context
            .icon_root
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
    }
}

fn install_context(
    state: &AppState,
    prototype: PrototypeStore,
    icon_root: Option<PathBuf>,
    runtime: &mut Runtime,
) {
    runtime.install_prototype_store(prototype.clone());
    *state.context.lock().expect("context lock") = Some(GameContext {
        prototype,
        icon_root,
    });
}

fn game_export_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("game-export");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir)
}

fn run_game(exe: &Path, config: &Path, args: &[&str], extra: &[String]) -> Result<(), String> {
    let mut command = std::process::Command::new(exe);
    command.args(args).arg("--config").arg(config);
    if !extra.is_empty() {
        command.args(extra);
    }
    let status = command
        .status()
        .map_err(|error| format!("启动游戏失败: {error}"))?;
    if !status.success() {
        return Err("游戏导出命令失败".to_string());
    }
    Ok(())
}

fn load_dump_impl(
    app: &AppHandle,
    state: &AppState,
    path: String,
) -> Result<ContextInfo, String> {
    let raw = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let dump: serde_json::Value = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
    let prototype = PrototypeStore::load(&dump).map_err(|error| error.to_string())?;
    // Icon root: a sibling "icons" directory when the dump came from a game
    // export (script-output/icons), otherwise no game icons.
    let icon_root = Path::new(&path)
        .parent()
        .map(|dir| dir.join("icons"))
        .filter(|dir| dir.is_dir());
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "runtime lock poisoned".to_string())?;
    install_context(state, prototype, icon_root, &mut runtime);
    let info = context_info(state);
    let _ = app.emit("context-loaded", &info);
    Ok(info)
}

fn load_game_context_impl(
    app: &AppHandle,
    state: &AppState,
    executable_path: &str,
    mod_dir: Option<&str>,
) -> Result<ContextInfo, String> {
    let exe = PathBuf::from(executable_path);
    if !exe.is_file() {
        return Err(format!("游戏可执行文件不存在: {executable_path}"));
    }
    let export = game_export_dir(app)?;
    let config = export.join("config.ini");
    let config_text = format!(
        "[path]\nwrite-data={}\n[general]\nlocale=zh-CN\n",
        export.to_string_lossy()
    );
    std::fs::write(&config, config_text).map_err(|error| error.to_string())?;

    let extra: Vec<String> = match mod_dir {
        Some(dir) => vec!["--mod-directory".to_string(), dir.to_string()],
        None => Vec::new(),
    };

    run_game(&exe, &config, &["--dump-data"], &extra)?;
    run_game(&exe, &config, &["--dump-prototype-locale"], &extra)?;
    run_game(&exe, &config, &["--dump-icon-sprites", "--disable-audio"], &extra)?;

    let script_output = export.join("script-output");
    let dump_path = script_output.join("data-raw-dump.json");
    if !dump_path.exists() {
        return Err(format!(
            "未找到导出数据: {}（请确认游戏已正确执行导出）",
            dump_path.display()
        ));
    }
    load_dump_impl(app, state, dump_path.to_string_lossy().to_string())
}

// ── Commands ──────────────────────────────────────────────────────

/// Load the embedded demo prototype store (idempotent; dev/demo helper).
#[tauri::command]
fn load_bundled_dump(state: State<'_, AppState>) -> Result<ContextInfo, String> {
    let dump: serde_json::Value =
        serde_json::from_str(DEMO_DUMP).map_err(|error| error.to_string())?;
    let prototype = PrototypeStore::load(&dump).map_err(|error| format!("dump: {error}"))?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "runtime lock poisoned".to_string())?;
    install_context(&state, prototype, None, &mut runtime);
    Ok(context_info(&state))
}

/// Run the Factorio executable to export data + locale + icon sprites, then
/// load the result as the game context.
#[tauri::command]
async fn load_game_context(
    app: AppHandle,
    executable_path: String,
    mod_dir: Option<String>,
) -> Result<ContextInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        load_game_context_impl(&app, &state, &executable_path, mod_dir.as_deref())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Load a pre-generated `data-raw-dump.json` as the game context.
#[tauri::command]
async fn load_dump(app: AppHandle, path: String) -> Result<ContextInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        load_dump_impl(&app, &state, path)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn get_context(state: State<'_, AppState>) -> ContextInfo {
    context_info(&state)
}

/// OS file dialog for the Factorio executable (game-context loading).
#[tauri::command]
async fn pick_game_executable(app: AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let picked = app
            .dialog()
            .file()
            .add_filter("Factorio 可执行文件", &["exe"])
            .blocking_pick_file();
        Ok(picked
            .and_then(|picked| picked.into_path().ok())
            .map(|path| path.to_string_lossy().to_string()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// OS file dialog for a pre-generated `data-raw-dump.json`.
#[tauri::command]
async fn pick_dump_file(app: AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let picked = app
            .dialog()
            .file()
            .add_filter("Factorio 数据 dump", &["json"])
            .blocking_pick_file();
        Ok(picked
            .and_then(|picked| picked.into_path().ok())
            .map(|path| path.to_string_lossy().to_string()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// OS folder dialog for a Factorio mod directory.
#[tauri::command]
async fn pick_mod_dir(app: AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let picked = app.dialog().file().blocking_pick_folder();
        Ok(picked
            .and_then(|picked| picked.into_path().ok())
            .map(|path| path.to_string_lossy().to_string()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Game icon PNG bytes for `<icon_root>/<ty>/<name>.png` (from
/// `--dump-icon-sprites`).  Falls back to `item/` and `entity/` so items and
/// their entity share icons where the dump only emitted one.
#[tauri::command]
fn icon(state: State<'_, AppState>, ty: String, name: String) -> Option<Vec<u8>> {
    let context = state.context.lock().ok()?;
    let root = context.as_ref()?.icon_root.as_ref()?;
    let candidates = [
        format!("{ty}/{name}.png"),
        format!("item/{name}.png"),
        format!("entity/{name}.png"),
    ];
    for candidate in candidates {
        let path = root.join(candidate);
        if path.is_file() {
            if let Ok(bytes) = std::fs::read(path) {
                return Some(bytes);
            }
        }
    }
    None
}

/// Searchable prototype catalog for the selector.
///
/// `kind`: item | fluid | recipe | module | machine | mining-machine |
/// generator | boiler | reactor | beacon | resource | entity | technology |
/// planet | surface | quality.
#[tauri::command]
fn catalog(
    state: State<'_, AppState>,
    kind: String,
    query: String,
    limit: usize,
) -> Vec<CatalogEntry> {
    let Ok(guard) = state.context.lock() else {
        return Vec::new();
    };
    let Some(context) = guard.as_ref() else {
        return Vec::new();
    };
    let limit = limit.clamp(1, 1000);
    let query = query.trim().to_lowercase();
    let mut out: Vec<CatalogEntry> = Vec::new();

    match kind.as_str() {
        "item" => ordered_group(
            &mut out,
            &context.prototype,
            PrototypeGroup::Item,
            "item",
            &query,
            limit,
        ),
        "fluid" => ordered_group(
            &mut out,
            &context.prototype,
            PrototypeGroup::Fluid,
            "fluid",
            &query,
            limit,
        ),
        "recipe" => ordered_group(
            &mut out,
            &context.prototype,
            PrototypeGroup::Recipe,
            "recipe",
            &query,
            limit,
        ),
        "technology" => ordered_group(
            &mut out,
            &context.prototype,
            PrototypeGroup::Technology,
            "technology",
            &query,
            limit,
        ),
        "planet" => ordered_group(
            &mut out,
            &context.prototype,
            PrototypeGroup::Planet,
            "planet",
            &query,
            limit,
        ),
        "surface" => ordered_group(
            &mut out,
            &context.prototype,
            PrototypeGroup::Surface,
            "surface",
            &query,
            limit,
        ),
        "module" => {
            for record in context.prototype.group(PrototypeGroup::Item) {
                if record.has("ModuleComponent") {
                    push_entry(
                        &mut out,
                        &context.prototype,
                        record,
                        "item",
                        None,
                        &query,
                        limit,
                    );
                }
            }
        }
        "machine" => entity_filtered(
            &mut out,
            &context.prototype,
            "CraftingMachineComponent",
            "entity",
            true,
            &query,
            limit,
        ),
        "mining-machine" => entity_filtered(
            &mut out,
            &context.prototype,
            "MiningDrillComponent",
            "entity",
            true,
            &query,
            limit,
        ),
        "generator" => entity_filtered(
            &mut out,
            &context.prototype,
            "GeneratorComponent",
            "entity",
            true,
            &query,
            limit,
        ),
        "boiler" => entity_filtered(
            &mut out,
            &context.prototype,
            "BoilerComponent",
            "entity",
            true,
            &query,
            limit,
        ),
        "reactor" => entity_filtered(
            &mut out,
            &context.prototype,
            "ReactorComponent",
            "entity",
            true,
            &query,
            limit,
        ),
        "beacon" => entity_filtered(
            &mut out,
            &context.prototype,
            "BeaconComponent",
            "entity",
            true,
            &query,
            limit,
        ),
        "entity" => entity_filtered(
            &mut out,
            &context.prototype,
            "EntityComponent",
            "entity",
            false,
            &query,
            limit,
        ),
        "resource" => {
            for record in context.prototype.group(PrototypeGroup::Entity) {
                if record.type_ == "resource" {
                    push_entry(
                        &mut out,
                        &context.prototype,
                        record,
                        "entity",
                        None,
                        &query,
                        limit,
                    );
                }
            }
        }
        "quality" => {
            for name in context.prototype.quality_order() {
                if out.len() >= limit {
                    break;
                }
                if !query.is_empty() && !name.to_lowercase().contains(&query) {
                    continue;
                }
                out.push(CatalogEntry {
                    name: name.clone(),
                    group: "quality".to_string(),
                    subgroup: String::new(),
                    icon_type: "quality".to_string(),
                    module_slots: None,
                });
            }
        }
        _ => {}
    }
    out
}

fn push_entry(
    out: &mut Vec<CatalogEntry>,
    store: &PrototypeStore,
    record: &PrototypeRecord,
    icon_type: &str,
    module_slots: Option<u16>,
    query: &str,
    limit: usize,
) {
    if out.len() >= limit {
        return;
    }
    if record
        .component::<PrototypeBaseComponent>()
        .map(|base| base.hidden)
        .unwrap_or(false)
    {
        return;
    }
    if record.name.starts_with("__") || record.name.ends_with("__") {
        return;
    }
    if !query.is_empty() && !record.name.to_lowercase().contains(query) {
        return;
    }
    let (group, subgroup) = subgroup_group(store, record);
    out.push(CatalogEntry {
        name: record.name.clone(),
        group,
        subgroup,
        icon_type: icon_type.to_string(),
        module_slots,
    });
}

fn ordered_group(
    out: &mut Vec<CatalogEntry>,
    store: &PrototypeStore,
    group: PrototypeGroup,
    icon_type: &str,
    query: &str,
    limit: usize,
) {
    let order = store.order_info();
    let Some(subgroups) = order.get(&group) else {
        return;
    };
    for (_, items_by_subgroup) in subgroups {
        for (_, items) in items_by_subgroup {
            for name in items {
                if let Some(record) = store.get(group, name) {
                    push_entry(out, store, record, icon_type, None, query, limit);
                }
            }
        }
    }
}

fn entity_filtered(
    out: &mut Vec<CatalogEntry>,
    store: &PrototypeStore,
    component: &str,
    icon_type: &str,
    module_slots: bool,
    query: &str,
    limit: usize,
) {
    for record in store.group(PrototypeGroup::Entity) {
        if !record.has(component) {
            continue;
        }
        let slots = if module_slots {
            record
                .component::<CraftingMachineComponent>()
                .and_then(|machine| machine.module_slots)
        } else {
            None
        };
        push_entry(out, store, record, icon_type, slots, query, limit);
    }
}

fn subgroup_group(store: &PrototypeStore, record: &PrototypeRecord) -> (String, String) {
    let Some(subgroup) = record
        .component::<PrototypeBaseComponent>()
        .and_then(|base| base.subgroup.clone())
    else {
        return ("other".to_string(), String::new());
    };
    let group = store
        .get(PrototypeGroup::ItemSubgroup, &subgroup)
        .and_then(|sub| sub.component::<ItemSubGroupComponent>())
        .map(|sub| sub.group.clone())
        .unwrap_or_else(|| "other".to_string());
    (group, subgroup)
}

/// Accept one user message and execute its side effects.
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
            execute_command(&app, &state, &mut runtime, command);
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

// ── Persistence ───────────────────────────────────────────────────

#[tauri::command]
async fn open_project_dialog(app: AppHandle) -> Result<Option<AppDocument>, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter("Metatorio 工程", &["json"])
        .blocking_pick_file();
    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|error| error.to_string())?;
    let state = app.state::<AppState>();
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "runtime lock poisoned".to_string())?;
    runtime
        .load_document_file(&path)
        .map_err(|error| error.to_string())?;
    if let Some(project) = runtime.state.ui.selected_project {
        if let Ok(mut paths) = state.project_paths.lock() {
            paths.insert(project, path.to_string_lossy().to_string());
        }
    }
    Ok(Some(runtime.state.document.clone()))
}

#[tauri::command]
async fn save_project_as_dialog(app: AppHandle) -> Result<Option<String>, String> {
    let picked = app
        .dialog()
        .file()
        .set_file_name("metatorio-project.json")
        .add_filter("Metatorio 工程", &["json"])
        .blocking_save_file();
    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|error| error.to_string())?;
    let path_string = path.to_string_lossy().to_string();
    let state = app.state::<AppState>();
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "runtime lock poisoned".to_string())?;
    let project = runtime
        .state
        .ui
        .selected_project
        .ok_or("没有选中的项目")?;
    runtime
        .save_document_file(project, &path)
        .map_err(|error| error.to_string())?;
    if let Ok(mut paths) = state.project_paths.lock() {
        paths.insert(project, path_string.clone());
    }
    Ok(Some(path_string))
}

/// Save to the remembered path; `Ok(None)` means no path yet (call
/// `save_project_as_dialog`).
#[tauri::command]
async fn save_project(app: AppHandle) -> Result<Option<String>, String> {
    let state = app.state::<AppState>();
    let project = state
        .runtime
        .lock()
        .map_err(|_| "runtime lock poisoned".to_string())?
        .state
        .ui
        .selected_project
        .ok_or("没有选中的项目")?;
    let path = state
        .project_paths
        .lock()
        .map_err(|_| "project paths lock poisoned".to_string())?
        .get(&project)
        .cloned();
    let Some(path) = path else {
        return Ok(None);
    };
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "runtime lock poisoned".to_string())?;
    runtime
        .save_document_file(project, &path)
        .map_err(|error| error.to_string())?;
    Ok(Some(path))
}

// ── Side effects ──────────────────────────────────────────────────

fn emit<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(error) = app.emit(event, payload) {
        eprintln!("failed to emit {event}: {error}");
    }
}

fn execute_command(
    app: &AppHandle,
    state: &AppState,
    runtime: &mut Runtime,
    command: &RuntimeCommand,
) {
    match command {
        RuntimeCommand::Recompute { project, factory } => {
            match runtime.solve_factory(*project, *factory) {
                Ok(result) => emit(app, "solve-result", result),
                Err(error) => emit(app, "solve-error", error.to_string()),
            }
        }
        RuntimeCommand::Persist { project, path } => {
            let path = path
                .clone()
                .or_else(|| state.project_paths.lock().ok()?.get(project).cloned());
            if let Some(path) = path {
                match runtime.save_document_file(*project, &path) {
                    Ok(()) => {
                        if let Ok(mut paths) = state.project_paths.lock() {
                            paths.insert(*project, path);
                        }
                    }
                    Err(error) => eprintln!("persist failed: {error}"),
                }
            }
            // Pathless persist with no remembered path is a no-op.
        }
        RuntimeCommand::LoadGameContext {
            executable_path,
            mod_path,
        } => {
            let result = load_game_context_impl(app, state, executable_path, mod_path.as_deref());
            if let Err(error) = result {
                emit(app, "context-error", error);
            }
        }
        RuntimeCommand::LoadCachedContext => {
            let cached = game_export_dir(app)
                .ok()
                .map(|dir| dir.join("script-output").join("data-raw-dump.json"))
                .filter(|path| path.exists())
                .map(|path| path.to_string_lossy().to_string());
            if let Some(path) = cached {
                if let Err(error) = load_dump_impl(app, state, path) {
                    emit(app, "context-error", error);
                }
            } else {
                emit(app, "context-error", "没有缓存的游戏数据".to_string());
            }
        }
        other => eprintln!("unhandled runtime command: {other:?}"),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            load_bundled_dump,
            load_game_context,
            load_dump,
            get_context,
            pick_game_executable,
            pick_dump_file,
            pick_mod_dir,
            icon,
            catalog,
            dispatch,
            get_document,
            get_ui_state,
            open_project_dialog,
            save_project_as_dialog,
            save_project,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
