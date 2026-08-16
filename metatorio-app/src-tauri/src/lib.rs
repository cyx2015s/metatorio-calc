//! Tauri adapter for the framework-independent `metatorio-runtime` layer.
//!
//! The frontend sends `AppMessage` values as JSON through the `dispatch`
//! command.  The runtime reducer returns a [`DispatchResult`]; side effects
//! (`RuntimeCommand`) are executed here — solving runs on a blocking worker
//! and its outcome is pushed to the frontend as a `solve-result` event.
//!
//! This layer also owns the game-context cache (multiple exported data sets,
//! each with its own prototype store + icon directory) and project file
//! persistence.  Contexts are cached under
//! `<app_data>/contexts/<content-hash>/`; projects pin the context they were
//! planned against via `ProjectDocument::context_id`.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use metatorio_core::{DualVar, IdWithQuality, Mechanic};
use metatorio_data::store::{PrototypeGroup, PrototypeRecord, PrototypeStore};
use metatorio_data::{
    BeaconComponent, CraftingMachineComponent, FluidComponent, ItemComponent, MiningDrillComponent,
    ModuleComponent, PrototypeBaseComponent, QualityComponent, RecipeComponent,
    ResourceEntityComponent,
};
use metatorio_runtime::{
    document::AppDocument,
    id::{FactoryId, MechanicId, ProjectId},
    message::{
        AppMessage, FactoryAction, MechanicAction, MiningMechanicAction, ModuleAction,
        ProjectAction, RecipeMechanicAction, RuntimeCommand,
    },
    solve::Runtime,
    state::{DispatchResult, UiState},
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

/// Minimal embedded game-data dump so the app can solve out of the box.
/// Replace with a real Factorio dump once data loading is wired to a
/// file dialog.
const DEMO_DUMP: &str = include_str!("../dumps/demo_dump.json");

// ── Managed state ─────────────────────────────────────────────────

pub struct AppState {
    runtime: Mutex<Runtime>,
    contexts: Mutex<ContextRegistry>,
    project_paths: Mutex<HashMap<ProjectId, String>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            runtime: Mutex::new(Runtime::new()),
            contexts: Mutex::new(ContextRegistry::default()),
            project_paths: Mutex::new(HashMap::new()),
        }
    }
}

/// Game contexts cached on disk under `<app_data>/contexts/<id>/`.
///
/// The registry keeps manifests only; loaded prototype stores live in
/// [`Runtime::contexts`] (single in-memory copy).  `id` is a content hash of
/// the raw dump, so identical exports dedupe and ids are stable across
/// machines.
#[derive(Default)]
struct ContextRegistry {
    dir: PathBuf,
    meta: HashMap<String, ContextMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContextMeta {
    id: String,
    name: String,
    source: String,
    created_at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextInfo {
    pub id: String,
    pub name: String,
    pub source: String,
    pub created_at: u64,
    pub loaded: bool,
    pub groups: Vec<GroupCount>,
    pub icon_root: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextList {
    pub active: Option<String>,
    pub contexts: Vec<ContextInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupCount {
    pub name: String,
    pub count: usize,
}

/// 目录索引条目：一次性下发到前端，筛选/分组/排序全部前端本地做。
#[derive(Debug, Clone, Serialize)]
pub struct IndexEntry {
    pub kind: String,
    pub name: String,
    pub group: String,
    pub subgroup: String,
    pub icon_type: String,
    pub module_slots: Option<u16>,
    /// 兼容性类别：machine→crafting_categories、recipe→categories、
    /// mining-machine→resource_categories、resource→category。
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogIndex {
    pub context_id: String,
    /// 可用品质（normal 起，按 order）。
    pub qualities: Vec<String>,
    pub entries: Vec<IndexEntry>,
}

/// 悬停详情（按需拉取 + 前端缓存）。
#[derive(Debug, Clone, Serialize)]
pub struct FlowAmount {
    pub kind: String,
    pub name: String,
    pub amount: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PrototypeDetail {
    pub name: String,
    pub kind: String,
    pub subgroup: Option<String>,
    pub order: String,
    pub hidden: bool,
    // item
    pub stack_size: Option<f64>,
    // recipe
    pub category: Option<String>,
    pub categories: Vec<String>,
    pub energy_required: Option<f64>,
    pub ingredients: Vec<FlowAmount>,
    pub results: Vec<FlowAmount>,
    // machine
    pub crafting_speed: Option<f64>,
    pub module_slots: Option<u16>,
    /// 机器允许的插件类别（空 = 不限制）。
    pub allowed_module_categories: Vec<String>,
    /// 焦耳/刻（功率）；前端换算为 W。
    pub energy_usage_j: Option<f64>,
    // beacon
    pub beacon_module_slots: Option<u16>,
    // fluid
    pub default_temperature: Option<f64>,
    // quality（kind = "quality"）
    pub quality_level: Option<u32>,
    pub quality_next: Option<String>,
    pub quality_next_probability: Option<f64>,
    pub quality_crafting_speed: Option<f64>,
    pub quality_module_speed: Option<f64>,
    pub quality_module_productivity: Option<f64>,
}

// ── Registry helpers ──────────────────────────────────────────────

impl ContextRegistry {
    fn store_dir(&self, id: &str) -> PathBuf {
        self.dir.join(id)
    }

    fn dump_path(&self, id: &str) -> PathBuf {
        self.store_dir(id).join("data-raw-dump.json")
    }

    fn manifest_path(&self, id: &str) -> PathBuf {
        self.store_dir(id).join("context.json")
    }

    fn icon_root(&self, id: &str) -> PathBuf {
        self.store_dir(id).join("icons")
    }

    /// Rebuild `meta` from the manifests on disk.  Dot-prefixed dirs are
    /// fast-delete trash: skipped here, purged on a background thread.
    fn scan(&mut self) {
        if self.dir.as_os_str().is_empty() {
            return;
        }
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
            else {
                continue;
            };
            if name.starts_with('.') {
                spawn_delete(path);
                continue;
            }
            if let Some(meta) = read_manifest(&path.join("context.json")) {
                self.meta.insert(name, meta);
            }
        }
    }

    /// Create the cache directory + manifest for a new context id.
    fn register(&mut self, id: String, name: String, source: String) {
        let created_at = now_secs();
        let meta = ContextMeta {
            id: id.clone(),
            name,
            source,
            created_at,
        };
        let _ = std::fs::create_dir_all(self.store_dir(&id));
        write_manifest(&self.manifest_path(&id), &meta);
        self.meta.insert(id, meta);
    }

    fn rename(&mut self, id: &str, name: String) -> Option<()> {
        let manifest_path = self.manifest_path(id);
        let meta = self.meta.get_mut(id)?;
        meta.name = name;
        write_manifest(&manifest_path, meta);
        Some(())
    }

    /// Fast delete: rename the cache dir to a dot-prefixed trash name and
    /// purge it on a background thread, so a huge `icons/` tree never blocks
    /// the UI.  Any leftover trash is picked up by [`Self::scan`] on startup.
    fn remove(&mut self, id: &str) {
        let Some(_meta) = self.meta.remove(id) else {
            return;
        };
        let store = self.store_dir(id);
        let trash = self.dir.join(format!(".trash-{id}"));
        // 清掉可能残留的同名 trash，避免 rename 失败。
        let _ = std::fs::remove_dir_all(&trash);
        if std::fs::rename(&store, &trash).is_ok() {
            spawn_delete(trash);
        } else {
            // rename 失败（跨卷等）：就地删，慢一点但保证删除。
            let _ = std::fs::remove_dir_all(&store);
        }
    }
}

/// Delete a directory tree off the UI thread (fire-and-forget).
fn spawn_delete(dir: PathBuf) {
    std::thread::spawn(move || {
        if let Err(error) = std::fs::remove_dir_all(&dir) {
            eprintln!("failed to purge {dir:?}: {error}");
        }
    });
}

fn read_manifest(path: &Path) -> Option<ContextMeta> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_manifest(path: &Path, meta: &ContextMeta) {
    if let Ok(json) = serde_json::to_string_pretty(meta) {
        let _ = std::fs::write(path, json);
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn context_id_of(raw: &[u8]) -> String {
    format!("{:016x}", xxhash_rust::xxh3::xxh3_64(raw))
}

// ── Context loading / registration ────────────────────────────────

fn ensure_context_loaded(
    state: &AppState,
    runtime: &mut Runtime,
    id: &str,
) -> Result<(), String> {
    if runtime.context_store_by_id(id).is_some() {
        return Ok(());
    }
    let dump_path = {
        let registry = state.contexts.lock().map_err(|_| "contexts 锁损坏".to_string())?;
        if !registry.meta.contains_key(id) {
            return Err(format!("上下文 {id} 不存在于缓存"));
        }
        registry.dump_path(id)
    };
    let raw = std::fs::read(&dump_path).map_err(|error| error.to_string())?;
    let dump: serde_json::Value = serde_json::from_slice(&raw).map_err(|error| error.to_string())?;
    let prototype = PrototypeStore::load(&dump).map_err(|error| error.to_string())?;
    runtime.install_context(id.to_string(), prototype);
    Ok(())
}

/// Register a new context (or reuse the cached one by content hash), persist
/// it, load its store and make it active.
/// 图标来源：导出路径用 Move（rename，同卷瞬间、缓存自包含不可变），
/// dump 导入路径用 Copy（用户目录不可 rename）。
enum IconImport {
    None,
    Move(PathBuf),
    Copy(PathBuf),
}

fn copy_dir(src: &Path, dst: &Path) {
    if !src.is_dir() {
        return;
    }
    let _ = std::fs::create_dir_all(dst);
    if let Ok(entries) = std::fs::read_dir(src) {
        for entry in entries.flatten() {
            let path = entry.path();
            let target = dst.join(entry.file_name());
            if path.is_dir() {
                copy_dir(&path, &target);
            } else {
                let _ = std::fs::copy(&path, &target);
            }
        }
    }
}

fn register_context(
    state: &AppState,
    runtime: &mut Runtime,
    name: String,
    source: String,
    raw: &[u8],
    icon: IconImport,
) -> Result<ContextInfo, String> {
    let id = context_id_of(raw);
    {
        let mut registry = state.contexts.lock().map_err(|_| "contexts 锁损坏".to_string())?;
        let is_new = !registry.meta.contains_key(&id);
        if is_new {
            registry.register(id.clone(), name, source);
            std::fs::write(registry.dump_path(&id), raw).map_err(|error| error.to_string())?;
        }
        // 图标：新上下文，或历史注册时缺图标（早期路径 bug 留下的缓存）
        // 都导入——重新导出同内容时 id 相同、注册被跳过，但图标仍需补齐。
        if !registry.icon_root(&id).is_dir() {
            match &icon {
                IconImport::None => {}
                IconImport::Move(src) => {
                    // 同卷 rename：把本次导出的类型目录整体移入缓存，之后
                    // 再次导出覆盖暂存目录也不会影响这个上下文。
                    let dst = registry.icon_root(&id);
                    if let Err(error) = std::fs::rename(src, &dst) {
                        eprintln!("移动图标失败（忽略，使用占位图标）: {error}");
                    }
                }
                IconImport::Copy(src) => copy_dir(src, &registry.icon_root(&id)),
            }
        }
    }
    ensure_context_loaded(state, runtime, &id)?;
    runtime.set_active_context(Some(id.clone()));
    context_info_of(state, runtime, &id).ok_or_else(|| "上下文信息缺失".to_string())
}

fn context_info_from(
    registry: &ContextRegistry,
    runtime: &Runtime,
    id: &str,
) -> Option<ContextInfo> {
    let meta = registry.meta.get(id)?.clone();
    let store = runtime.context_store_by_id(id);
    let icon_root = registry.icon_root(id);
    Some(ContextInfo {
        id: meta.id,
        name: meta.name,
        source: meta.source,
        created_at: meta.created_at,
        loaded: store.is_some(),
        groups: store
            .map(|store| {
                store
                    .groups
                    .iter()
                    .map(|(group, records)| GroupCount {
                        name: format!("{group:?}"),
                        count: records.len(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        icon_root: icon_root.is_dir().then(|| icon_root.to_string_lossy().to_string()),
        active: runtime.active_context() == Some(id),
    })
}

fn context_info_of(state: &AppState, runtime: &Runtime, id: &str) -> Option<ContextInfo> {
    let registry = state.contexts.lock().ok()?;
    context_info_from(&registry, runtime, id)
}

/// 构建上下文列表。调用方若已持有 runtime 锁，请用
/// [`context_list_with`]（避免 std Mutex 不可重入导致死锁）。
fn context_list(state: &AppState) -> ContextList {
    let runtime = state.runtime.lock().ok();
    let Some(runtime) = runtime.as_ref() else {
        return ContextList {
            active: None,
            contexts: Vec::new(),
        };
    };
    context_list_with(runtime, state)
}

fn context_list_with(runtime: &Runtime, state: &AppState) -> ContextList {
    let registry = state.contexts.lock().ok();
    let Some(registry) = registry.as_ref() else {
        return ContextList {
            active: None,
            contexts: Vec::new(),
        };
    };
    let active = runtime.active_context().map(str::to_string);
    let mut contexts: Vec<ContextInfo> = registry
        .meta
        .keys()
        .filter_map(|id| context_info_from(registry, runtime, id))
        .collect();
    contexts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    ContextList { active, contexts }
}

/// 调用方持有 runtime 锁时传 `Some(runtime)`，否则传 `None`。
fn emit_contexts_changed(app: &AppHandle, state: &AppState, runtime: Option<&Runtime>) {
    let list = match runtime {
        Some(runtime) => context_list_with(runtime, state),
        None => context_list(state),
    };
    if let Err(error) = app.emit("contexts-changed", list) {
        eprintln!("failed to emit contexts-changed: {error}");
    }
}

// ── Game export (executable) ──────────────────────────────────────

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
    let raw = std::fs::read(&dump_path).map_err(|error| error.to_string())?;
    let name = mod_dir
        .and_then(|dir| Path::new(dir).file_name())
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| "vanilla".to_string());
    let source = format!(
        "exe: {executable_path}{}",
        mod_dir.map(|dir| format!(", mods: {dir}")).unwrap_or_default()
    );
    let icon_src = script_output.clone();
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "runtime lock poisoned".to_string())?;
    register_context(
        state,
        &mut runtime,
        name,
        source,
        &raw,
        IconImport::Move(icon_src),
    )
}

// ── Commands ──────────────────────────────────────────────────────

/// Load the embedded demo prototype store as a context (idempotent by hash).
#[tauri::command]
fn load_bundled_dump(app: AppHandle) -> Result<ContextInfo, String> {
    let state = app.state::<AppState>();
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "runtime lock poisoned".to_string())?;
    let info = register_context(
        &state,
        &mut runtime,
        "内置示例".to_string(),
        "embedded demo".to_string(),
        DEMO_DUMP.as_bytes(),
        IconImport::None,
    )?;
    emit_contexts_changed(&app, &state, Some(&runtime));
    Ok(info)
}

/// Run the Factorio executable to export data + locale + icon sprites, then
/// cache and activate the result as a context.
#[tauri::command]
async fn load_game_context(
    app: AppHandle,
    executable_path: String,
    mod_dir: Option<String>,
) -> Result<ContextInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let info = load_game_context_impl(&app, &state, &executable_path, mod_dir.as_deref())?;
        emit_contexts_changed(&app, &state, None);
        Ok(info)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Load a pre-generated `data-raw-dump.json` as a cached context.
#[tauri::command]
async fn load_dump(app: AppHandle, path: String) -> Result<ContextInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let raw = std::fs::read(&path).map_err(|error| error.to_string())?;
        let name = Path::new(&path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("dump")
            .to_string();
        let source = format!("dump: {path}");
        // 图标根：优先 dump 旁的 icons/（旧约定），否则 dump 所在目录本身
        // （导出时类型目录直接位于 script-output 根下）。
        let icon = match Path::new(&path).parent() {
            Some(parent) => {
                let sibling = parent.join("icons");
                if sibling.is_dir() {
                    IconImport::Copy(sibling)
                } else if parent.join("item").is_dir() {
                    IconImport::Copy(parent.to_path_buf())
                } else {
                    IconImport::None
                }
            }
            None => IconImport::None,
        };
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "runtime lock poisoned".to_string())?;
        let info = register_context(&state, &mut runtime, name, source, &raw, icon)?;
        emit_contexts_changed(&app, &state, Some(&runtime));
        Ok(info)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// All cached contexts + the active context id.
#[tauri::command]
fn list_contexts(state: State<'_, AppState>) -> ContextList {
    context_list(&state)
}

/// Activate a context (loading its store from cache on demand).
#[tauri::command]
async fn set_active_context(app: AppHandle, id: Option<String>) -> Result<ContextList, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "runtime lock poisoned".to_string())?;
        if let Some(id) = &id {
            ensure_context_loaded(&state, &mut runtime, id)?;
        }
        runtime.set_active_context(id);
        emit_contexts_changed(&app, &state, Some(&runtime));
        Ok(context_list_with(&runtime, &state))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rename_context(app: AppHandle, id: String, name: String) -> Result<ContextList, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("名称不能为空".to_string());
        }
        {
            let mut registry = state
                .contexts
                .lock()
                .map_err(|_| "contexts 锁损坏".to_string())?;
            registry
                .rename(&id, name)
                .ok_or_else(|| format!("上下文 {id} 不存在"))?;
        }
        emit_contexts_changed(&app, &state, None);
        Ok(context_list(&state))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn delete_context(app: AppHandle, id: String) -> Result<ContextList, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        {
            let runtime = state
                .runtime
                .lock()
                .map_err(|_| "runtime lock poisoned".to_string())?;
            let referenced = runtime
                .state
                .document
                .projects
                .iter()
                .any(|project| project.context_id.as_deref() == Some(id.as_str()));
            if referenced {
                return Err("有项目正在引用该上下文，请先解除关联".to_string());
            }
        }
        {
            let mut registry = state
                .contexts
                .lock()
                .map_err(|_| "contexts 锁损坏".to_string())?;
            registry.remove(&id);
        }
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "runtime lock poisoned".to_string())?;
        runtime.remove_context(&id);
        emit_contexts_changed(&app, &state, Some(&runtime));
        Ok(context_list_with(&runtime, &state))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Game icon PNG bytes for the *effective* context's `<icons>/<ty>/<name>.png`
/// (from `--dump-icon-sprites`)。图标在注册时已移入/拷入缓存，缓存自包含。
#[tauri::command]
fn icon(state: State<'_, AppState>, ty: String, name: String) -> Option<Vec<u8>> {
    let runtime = state.runtime.lock().ok()?;
    let id = runtime.effective_context_id()?;
    let cache_root = {
        let registry = state.contexts.lock().ok()?;
        registry.icon_root(&id)
    };
    if !cache_root.is_dir() {
        return None;
    }
    let candidates: Vec<String> = if ty == "quality" {
        // 品质图标只有 quality/ 目录；回退到 item/entity 会显示错误的物品图标。
        vec![format!("quality/{name}.png")]
    } else {
        vec![
            format!("{ty}/{name}.png"),
            format!("item/{name}.png"),
            format!("entity/{name}.png"),
        ]
    };
    for candidate in candidates {
        let path = cache_root.join(candidate);
        if path.is_file() {
            if let Ok(bytes) = std::fs::read(path) {
                return Some(bytes);
            }
        }
    }
    None
}

/// 全量目录索引（含 order fallback 排序）：一次拉取，前端本地筛选/分组。
#[tauri::command]
fn catalog_index(state: State<'_, AppState>) -> Result<CatalogIndex, String> {
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "runtime lock poisoned".to_string())?;
    let Some(id) = runtime.effective_context_id() else {
        return Ok(CatalogIndex {
            context_id: String::new(),
            qualities: Vec::new(),
            entries: Vec::new(),
        });
    };
    ensure_context_loaded(&state, &mut runtime, &id)?;
    let store = runtime.context_store_by_id(&id).ok_or("上下文未载入")?;
    Ok(CatalogIndex {
        context_id: id,
        qualities: store.quality_order().to_vec(),
        entries: catalog_index_from_store(store),
    })
}

fn catalog_index_from_store(store: &PrototypeStore) -> Vec<IndexEntry> {
    let mut out: Vec<IndexEntry> = Vec::new();

    // 有 order_info 的组：大组 → 小组 → 条目（recipe/entity fallback 已在
    // order_info 中生效）
    let ordered = [
        ("item", PrototypeGroup::Item, "item"),
        ("fluid", PrototypeGroup::Fluid, "fluid"),
        ("recipe", PrototypeGroup::Recipe, "recipe"),
        ("technology", PrototypeGroup::Technology, "technology"),
        ("planet", PrototypeGroup::Planet, "planet"),
        ("surface", PrototypeGroup::Surface, "surface"),
    ];
    for (kind, group, icon_type) in ordered {
        let Some(order) = store.order_info().get(&group) else {
            continue;
        };
        for (big, subgroups) in order {
            for (sub, names) in subgroups {
                for name in names {
                    let categories = if kind == "recipe" {
                        store
                            .get(PrototypeGroup::Recipe, name)
                            .and_then(|record| record.component::<RecipeComponent>())
                            .map(effective_recipe_categories)
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    out.push(IndexEntry {
                        kind: kind.to_string(),
                        name: name.clone(),
                        group: big.clone(),
                        subgroup: sub.clone(),
                        icon_type: icon_type.to_string(),
                        module_slots: None,
                        categories,
                    });
                }
            }
        }
    }

    // 实体类：Entity 的 order_info（含 fallback）里按组件过滤
    let entity_kinds = [
        ("machine", "CraftingMachineComponent", true),
        ("mining-machine", "MiningDrillComponent", true),
        ("generator", "GeneratorComponent", true),
        ("boiler", "BoilerComponent", true),
        ("reactor", "ReactorComponent", true),
        ("beacon", "BeaconComponent", true),
        ("entity", "EntityComponent", false),
    ];
    for (kind, component, want_slots) in entity_kinds {
        let Some(order) = store.order_info().get(&PrototypeGroup::Entity) else {
            continue;
        };
        for (big, subgroups) in order {
            for (sub, names) in subgroups {
                for name in names {
                    let Some(record) = store.get(PrototypeGroup::Entity, name) else {
                        continue;
                    };
                    if !record.has(component) {
                        continue;
                    }
                    let slots = if want_slots {
                        record
                            .component::<CraftingMachineComponent>()
                            .and_then(|machine| machine.module_slots)
                    } else {
                        None
                    };
                    let categories = match kind {
                        "machine" => record
                            .component::<CraftingMachineComponent>()
                            .map(|machine| machine.crafting_categories.clone())
                            .unwrap_or_default(),
                        "mining-machine" => record
                            .component::<MiningDrillComponent>()
                            .map(|drill| drill.resource_categories.clone())
                            .unwrap_or_default(),
                        _ => Vec::new(),
                    };
                    out.push(IndexEntry {
                        kind: kind.to_string(),
                        name: name.clone(),
                        group: big.clone(),
                        subgroup: sub.clone(),
                        icon_type: "entity".to_string(),
                        module_slots: slots,
                        categories,
                    });
                }
            }
        }
    }

    // module：Item order_info 过滤 ModuleComponent
    if let Some(order) = store.order_info().get(&PrototypeGroup::Item) {
        for (big, subgroups) in order {
            for (sub, names) in subgroups {
                for name in names {
                    let Some(record) = store.get(PrototypeGroup::Item, name) else {
                        continue;
                    };
                    let Some(module) = record.component::<ModuleComponent>() else {
                        continue;
                    };
                    out.push(IndexEntry {
                        kind: "module".to_string(),
                        name: name.clone(),
                        group: big.clone(),
                        subgroup: sub.clone(),
                        icon_type: "item".to_string(),
                        module_slots: None,
                        categories: if module.category.is_empty() {
                            Vec::new()
                        } else {
                            vec![module.category.clone()]
                        },
                    });
                }
            }
        }
    }

    // resource：Entity order_info 过滤 type_ == "resource"
    if let Some(order) = store.order_info().get(&PrototypeGroup::Entity) {
        for (big, subgroups) in order {
            for (sub, names) in subgroups {
                for name in names {
                    let Some(record) = store.get(PrototypeGroup::Entity, name) else {
                        continue;
                    };
                    if record.type_ != "resource" {
                        continue;
                    }
                    let categories = record
                        .component::<ResourceEntityComponent>()
                        .map(|resource| vec![effective_resource_category(resource)])
                        .unwrap_or_default();
                    out.push(IndexEntry {
                        kind: "resource".to_string(),
                        name: name.clone(),
                        group: big.clone(),
                        subgroup: sub.clone(),
                        icon_type: "entity".to_string(),
                        module_slots: None,
                        categories,
                    });
                }
            }
        }
    }

    // quality
    for name in store.quality_order() {
        out.push(IndexEntry {
            kind: "quality".to_string(),
            name: name.clone(),
            group: "quality".to_string(),
            subgroup: String::new(),
            icon_type: "quality".to_string(),
            module_slots: None,
            categories: Vec::new(),
        });
    }

    out
}

/// 悬停详情：按需拉取，前端缓存。
#[tauri::command]
fn prototype_detail(
    state: State<'_, AppState>,
    kind: String,
    name: String,
) -> Result<Option<PrototypeDetail>, String> {
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "runtime lock poisoned".to_string())?;
    let Some(id) = runtime.effective_context_id() else {
        return Ok(None);
    };
    ensure_context_loaded(&state, &mut runtime, &id)?;
    let store = runtime.context_store_by_id(&id).ok_or("上下文未载入")?;
    let record = match kind.as_str() {
        "item" | "module" => store.get(PrototypeGroup::Item, &name),
        "fluid" => store.get(PrototypeGroup::Fluid, &name),
        "recipe" => store.get(PrototypeGroup::Recipe, &name),
        "technology" => store.get(PrototypeGroup::Technology, &name),
        "planet" => store.get(PrototypeGroup::Planet, &name),
        "surface" => store.get(PrototypeGroup::Surface, &name),
        "quality" => store.get(PrototypeGroup::Quality, &name),
        _ => store.get(PrototypeGroup::Entity, &name),
    };
    let Some(record) = record else {
        return Ok(None);
    };
    let mut detail = PrototypeDetail {
        name: record.name.clone(),
        kind: kind.clone(),
        ..Default::default()
    };
    if let Some(base) = record.component::<PrototypeBaseComponent>() {
        detail.subgroup = base.subgroup.clone();
        detail.order = base.order.clone();
        detail.hidden = base.hidden;
    }
    if let Some(item) = record.component::<ItemComponent>() {
        detail.stack_size = Some(item.stack_size as f64);
    }
    if let Some(recipe) = record.component::<RecipeComponent>() {
        detail.categories = effective_recipe_categories(recipe);
        detail.category = Some(detail.categories.join(", "));
        detail.energy_required = Some(recipe.energy_required);
        detail.ingredients = recipe.ingredients.iter().map(ingredient_flow).collect();
        detail.results = recipe.results.iter().map(product_flow).collect();
    }
    if let Some(machine) = record.component::<CraftingMachineComponent>() {
        detail.crafting_speed = Some(machine.crafting_speed);
        detail.module_slots = machine.module_slots;
        detail.allowed_module_categories = machine
            .allowed_module_categories
            .clone()
            .unwrap_or_default();
        detail.energy_usage_j = Some(machine.energy_usage.amount);
        detail.categories = machine.crafting_categories.clone();
    }
    if let Some(drill) = record.component::<MiningDrillComponent>() {
        detail.categories = drill.resource_categories.clone();
        detail.module_slots = drill.module_slots;
        detail.allowed_module_categories = drill
            .allowed_module_categories
            .clone()
            .unwrap_or_default();
    }
    if let Some(beacon) = record.component::<BeaconComponent>() {
        detail.beacon_module_slots = Some(beacon.module_slots);
        detail.allowed_module_categories = beacon
            .allowed_module_categories
            .clone()
            .unwrap_or_default();
    }
    if let Some(resource) = record.component::<ResourceEntityComponent>() {
        detail.categories = vec![effective_resource_category(resource)];
    }
    if let Some(fluid) = record.component::<FluidComponent>() {
        detail.default_temperature = Some(fluid.default_temperature);
    }
    if let Some(quality) = record.component::<QualityComponent>() {
        detail.quality_level = Some(quality.level);
        detail.quality_next = quality.next.clone();
        detail.quality_next_probability = Some(quality.next_probability);
        detail.quality_crafting_speed = quality.crafting_machine_speed_multiplier;
        detail.quality_module_speed = quality.module_speed_multiplier;
        detail.quality_module_productivity = quality.module_productivity_multiplier;
    }
    Ok(Some(detail))
}

fn ingredient_flow(ingredient: &metatorio_data::types::Ingredient) -> FlowAmount {
    use metatorio_data::types::Ingredient;
    match ingredient {
        Ingredient::Item(item) => FlowAmount {
            kind: "item".to_string(),
            name: item.name.clone(),
            amount: item.amount as f64,
        },
        Ingredient::Fluid(fluid) => FlowAmount {
            kind: "fluid".to_string(),
            name: fluid.name.clone(),
            amount: fluid.amount,
        },
    }
}

fn product_flow(product: &metatorio_data::types::Product) -> FlowAmount {
    use metatorio_data::types::Product;
    match product {
        Product::Item(item) => FlowAmount {
            kind: "item".to_string(),
            name: item.name.clone(),
            amount: item.amount.unwrap_or(0) as f64,
        },
        Product::Fluid(fluid) => FlowAmount {
            kind: "fluid".to_string(),
            name: fluid.name.clone(),
            amount: fluid.amount.unwrap_or(0.0),
        },
    }
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

/// 机器有效插件槽位（基础 + 品质加成；制造机/采矿机）。
fn effective_module_slots(
    store: &PrototypeStore,
    machine_id: &str,
    machine_quality: &str,
) -> usize {
    let Some(entity) = store.entity(machine_id) else {
        return 0;
    };
    let base = entity
        .component::<CraftingMachineComponent>()
        .and_then(|machine| machine.module_slots)
        .or_else(|| {
            entity
                .component::<MiningDrillComponent>()
                .and_then(|drill| drill.module_slots)
        })
        .unwrap_or(0) as usize;
    let bonus = entity
        .component::<CraftingMachineComponent>()
        .and_then(|machine| machine.module_slots_quality_bonus.get(machine_quality))
        .copied()
        .unwrap_or(0) as usize;
    base + bonus
}

/// 机器变化后按槽位上限钳制模块数量（超出直接截断，经 reducer 落盘）。
fn clamp_modules(
    state: &AppState,
    runtime: &mut Runtime,
    project: ProjectId,
    factory: FactoryId,
    mechanic: MechanicId,
) -> Result<(), String> {
    let context_id = runtime
        .state
        .project(project)
        .map_err(|error| error.to_string())?
        .context_id
        .clone()
        .or_else(|| runtime.active_context().map(str::to_string));
    let Some(context_id) = context_id else {
        return Ok(());
    };
    ensure_context_loaded(state, runtime, &context_id)?;
    let store = runtime
        .context_store_by_id(&context_id)
        .ok_or_else(|| "上下文未载入".to_string())?
        .clone();

    let entry = runtime
        .state
        .factory(project, factory)
        .map_err(|error| error.to_string())?
        .mechanics
        .iter()
        .find(|entry| entry.id == mechanic)
        .cloned()
        .ok_or_else(|| "机制不存在".to_string())?;

    let (module_count, max) = match &entry.mechanic {
        Mechanic::Recipe(recipe) => (
            recipe.module_config.modules.len(),
            effective_module_slots(&store, &recipe.machine.id, &recipe.machine.quality),
        ),
        Mechanic::Mining(mining) => (
            mining.module_config.modules.len(),
            effective_module_slots(&store, &mining.machine.id, &mining.machine.quality),
        ),
        _ => return Ok(()),
    };
    if module_count <= max {
        return Ok(());
    }
    let action = match &entry.mechanic {
        Mechanic::Recipe(_) => MechanicAction::Recipe(RecipeMechanicAction::Module(
            ModuleAction::ClampModules { max },
        )),
        _ => MechanicAction::Mining(MiningMechanicAction::Module(ModuleAction::ClampModules {
            max,
        })),
    };
    runtime
        .dispatch(AppMessage::Factory {
            project,
            factory,
            action: FactoryAction::Mechanic { mechanic, action },
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn emit<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(error) = app.emit(event, payload) {
        eprintln!("failed to emit {event}: {error}");
    }
}

/// 在实体组里挑选一台机器：优先项目规划偏好的机器偏好，其次按给定
/// 排序分数（如 crafting_speed）取最优；都不满足返回 None。
fn pick_entity<F: Fn(&metatorio_data::store::PrototypeRecord) -> bool, S: Fn(&metatorio_data::store::PrototypeRecord) -> f64>(
    store: &PrototypeStore,
    prefs: &[IdWithQuality],
    matches: F,
    score: S,
) -> Option<String> {
    let mut candidates: Vec<(&PrototypeRecord, f64)> = store
        .group(PrototypeGroup::Entity)
        .filter(|record| matches(record))
        .map(|record| (record, score(record)))
        .collect();
    for pref in prefs {
        if let Some((record, _)) = candidates.iter().find(|(record, _)| record.name == pref.id) {
            return Some(record.name.clone());
        }
    }
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.name.cmp(&b.0.name))
    });
    Some(candidates[0].0.name.clone())
}

fn categories_overlap(required: &[String], available: &[String]) -> bool {
    required.is_empty() || available.iter().any(|available| required.contains(available))
}

/// 配方有效类别：空数组 = 默认 `["crafting"]`，不是"任意机器都能造"。
fn effective_recipe_categories(recipe: &RecipeComponent) -> Vec<String> {
    let categories = recipe.categories.clone().unwrap_or_default();
    if categories.is_empty() {
        vec!["crafting".to_string()]
    } else {
        categories
    }
}

/// 资源有效类别：空 = 默认 `"basic-solid"`。
fn effective_resource_category(resource: &ResourceEntityComponent) -> String {
    if resource.category.is_empty() {
        "basic-solid".to_string()
    } else {
        resource.category.clone()
    }
}

fn quality_level_of(qualities: &[String], name: &str) -> usize {
    qualities.iter().position(|candidate| candidate == name).unwrap_or(0)
}

fn flow_quality_level(qualities: &[String], flow: &DualVar) -> usize {
    let name = match flow {
        DualVar::Item(id) | DualVar::Entity(id) => &id.quality,
        _ => return 0,
    };
    quality_level_of(qualities, name)
}

fn mechanic_quality_level(qualities: &[String], mechanic: &Mechanic) -> usize {
    let mut ids: Vec<&IdWithQuality> = match mechanic {
        Mechanic::Recipe(mechanic) => vec![&mechanic.recipe, &mechanic.machine],
        Mechanic::Mining(mechanic) => vec![&mechanic.machine],
        Mechanic::Spoil(mechanic) => vec![&mechanic.item],
        Mechanic::Plant(mechanic) => vec![&mechanic.seed],
        Mechanic::ItemFuel(mechanic) => vec![&mechanic.item],
        Mechanic::ItemLaunch(mechanic) => vec![&mechanic.item],
        Mechanic::Generator(mechanic) => vec![&mechanic.generator],
        Mechanic::Boiler(mechanic) => vec![&mechanic.boiler],
        Mechanic::Reactor(mechanic) => vec![&mechanic.reactor],
        _ => Vec::new(),
    };
    if let Mechanic::Recipe(mechanic) = mechanic {
        ids.extend(mechanic.module_config.modules.iter());
    }
    if let Mechanic::Mining(mechanic) = mechanic {
        ids.extend(mechanic.module_config.modules.iter());
    }
    ids.iter()
        .map(|id| quality_level_of(qualities, &id.quality))
        .max()
        .unwrap_or(0)
}

/// 项目品质上限自动提升：文档中出现高于当前上限的品质时（目标/外部输入/
/// 机制），把 `ProjectSettings.quality_limit` 提升到该品质。这样"显式要求
/// uncommon 目标"不会被默认的 normal 上限静默判死。
fn ensure_quality_limit(
    state: &AppState,
    runtime: &mut Runtime,
    project: ProjectId,
) -> Result<(), String> {
    let context_id = runtime
        .state
        .project(project)
        .map_err(|error| error.to_string())?
        .context_id
        .clone()
        .or_else(|| runtime.active_context().map(str::to_string));
    let Some(context_id) = context_id else {
        return Ok(());
    };
    ensure_context_loaded(state, runtime, &context_id)?;
    let qualities = runtime
        .context_store_by_id(&context_id)
        .map(|store| store.quality_order().to_vec())
        .unwrap_or_default();
    if qualities.len() <= 1 {
        return Ok(());
    }

    let (all_accessible, current_limit) = {
        let project_doc = runtime
            .state
            .project(project)
            .map_err(|error| error.to_string())?;
        (
            project_doc.settings.all_accessible,
            project_doc.settings.quality_limit.clone(),
        )
    };
    if all_accessible {
        return Ok(());
    }
    let current_level = current_limit
        .as_deref()
        .map(|quality| quality_level_of(&qualities, quality))
        .unwrap_or(0);

    let mut max_level = current_level;
    {
        let project_doc = runtime
            .state
            .project(project)
            .map_err(|error| error.to_string())?;
        for factory in &project_doc.factories {
            for target in &factory.targets {
                max_level = max_level.max(flow_quality_level(&qualities, &target.flow));
            }
            for input in &factory.external_inputs {
                max_level = max_level.max(flow_quality_level(&qualities, &input.flow));
            }
            for entry in &factory.mechanics {
                max_level = max_level.max(mechanic_quality_level(&qualities, &entry.mechanic));
            }
        }
    }
    if max_level > current_level {
        let quality = qualities[max_level].clone();
        runtime
            .dispatch(AppMessage::Project {
                project,
                action: ProjectAction::SetQualityLimit {
                    quality: Some(quality),
                },
            })
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// 配方/资源变化后的机器兼容性校验与回退：
/// - 当前机器兼容（类别匹配）→ 不动；
/// - 不兼容或未设置 → 挑选默认机器（项目规划偏好优先，其次最高 crafting_speed），
///   通过 reducer 重新 SetMachine（保持原品质）。
fn ensure_machine_compat(
    state: &AppState,
    runtime: &mut Runtime,
    project: ProjectId,
    factory: FactoryId,
    mechanic: MechanicId,
) -> Result<(), String> {
    let context_id = runtime
        .state
        .project(project)
        .map_err(|error| error.to_string())?
        .context_id
        .clone()
        .or_else(|| runtime.active_context().map(str::to_string));
    let Some(context_id) = context_id else {
        return Ok(()); // 没有上下文时无从校验
    };
    ensure_context_loaded(state, runtime, &context_id)?;
    let store = runtime
        .context_store_by_id(&context_id)
        .ok_or_else(|| "上下文未载入".to_string())?
        .clone();
    let prefs = runtime
        .state
        .project(project)
        .map_err(|error| error.to_string())?
        .planning
        .machine_preferences
        .clone();

    let entry = runtime
        .state
        .factory(project, factory)
        .map_err(|error| error.to_string())?
        .mechanics
        .iter()
        .find(|entry| entry.id == mechanic)
        .cloned()
        .ok_or_else(|| "机制不存在".to_string())?;

    match &entry.mechanic {
        Mechanic::Recipe(recipe) => {
            let recipe_categories = store
                .get(PrototypeGroup::Recipe, &recipe.recipe.id)
                .and_then(|record| record.component::<RecipeComponent>())
                .map(effective_recipe_categories)
                .unwrap_or_default();
            let machine_ok = !recipe.machine.id.is_empty()
                && store
                    .get(PrototypeGroup::Entity, &recipe.machine.id)
                    .and_then(|record| record.component::<CraftingMachineComponent>())
                    .is_some_and(|machine| {
                        categories_overlap(&recipe_categories, &machine.crafting_categories)
                    });
            if machine_ok {
                return Ok(());
            }
            let pick = pick_entity(
                &store,
                &prefs,
                |record| {
                    record.component::<CraftingMachineComponent>().is_some_and(|machine| {
                        categories_overlap(&recipe_categories, &machine.crafting_categories)
                    })
                },
                |record| {
                    record
                        .component::<CraftingMachineComponent>()
                        .map(|machine| machine.crafting_speed)
                        .unwrap_or(0.0)
                },
            );
            if let Some(machine) = pick {
                let machine = IdWithQuality::new(machine, &recipe.machine.quality);
                runtime
                    .dispatch(AppMessage::Factory {
                        project,
                        factory,
                        action: FactoryAction::Mechanic {
                            mechanic,
                            action: MechanicAction::Recipe(RecipeMechanicAction::SetMachine {
                                machine,
                            }),
                        },
                    })
                    .map_err(|error| error.to_string())?;
            }
        }
        Mechanic::Mining(mining) => {
            let resource_category = store
                .get(PrototypeGroup::Entity, &mining.resource)
                .and_then(|record| record.component::<ResourceEntityComponent>())
                .map(effective_resource_category)
                .unwrap_or_default();
            let machine_ok = !mining.machine.id.is_empty()
                && store
                    .get(PrototypeGroup::Entity, &mining.machine.id)
                    .and_then(|record| record.component::<MiningDrillComponent>())
                    .is_some_and(|drill| drill.resource_categories.contains(&resource_category));
            if machine_ok {
                return Ok(());
            }
            let pick = pick_entity(
                &store,
                &prefs,
                |record| {
                    record
                        .component::<MiningDrillComponent>()
                        .is_some_and(|drill| drill.resource_categories.contains(&resource_category))
                },
                |_| 0.0,
            );
            if let Some(machine) = pick {
                let machine = IdWithQuality::new(machine, &mining.machine.quality);
                runtime
                    .dispatch(AppMessage::Factory {
                        project,
                        factory,
                        action: FactoryAction::Mechanic {
                            mechanic,
                            action: MechanicAction::Mining(MiningMechanicAction::SetMachine {
                                machine,
                            }),
                        },
                    })
                    .map_err(|error| error.to_string())?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn execute_command(
    app: &AppHandle,
    state: &AppState,
    runtime: &mut Runtime,
    command: &RuntimeCommand,
) {
    match command {
        RuntimeCommand::Recompute { project, factory } => {
            // Make sure the project's context store is in memory first.
            let context_id = runtime
                .state
                .project(*project)
                .ok()
                .and_then(|project| project.context_id.clone())
                .or_else(|| runtime.active_context().map(str::to_string));
            if let Some(id) = context_id {
                if let Err(error) = ensure_context_loaded(state, runtime, &id) {
                    emit(app, "solve-error", error);
                    return;
                }
            }
            match runtime.solve_factory(*project, *factory) {
                Ok(result) => emit(app, "solve-result", result),
                Err(error) => emit(app, "solve-error", error.to_string()),
            }
        }
        RuntimeCommand::EnsureMachineCompat {
            project,
            factory,
            mechanic,
        } => {
            if let Err(error) = ensure_machine_compat(state, runtime, *project, *factory, *mechanic)
            {
                eprintln!("machine compat fallback failed: {error}");
            }
        }
        RuntimeCommand::EnsureQualityLimit { project } => {
            if let Err(error) = ensure_quality_limit(state, runtime, *project) {
                eprintln!("quality limit auto-raise failed: {error}");
            }
        }
        RuntimeCommand::ClampModules {
            project,
            factory,
            mechanic,
        } => {
            if let Err(error) = clamp_modules(state, runtime, *project, *factory, *mechanic) {
                eprintln!("module clamp failed: {error}");
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
            match result {
                Ok(_) => emit_contexts_changed(app, state, Some(runtime)),
                Err(error) => emit(app, "context-error", error),
            }
        }
        RuntimeCommand::LoadCachedContext => {
            // 恢复最近创建的上下文。
            let newest = state
                .contexts
                .lock()
                .ok()
                .and_then(|registry| {
                    registry
                        .meta
                        .values()
                        .max_by_key(|meta| meta.created_at)
                        .map(|meta| meta.id.clone())
                });
            match newest {
                Some(id) => match ensure_context_loaded(state, runtime, &id) {
                    Ok(()) => {
                        runtime.set_active_context(Some(id));
                        emit_contexts_changed(app, state, Some(runtime));
                    }
                    Err(error) => emit(app, "context-error", error),
                },
                None => emit(app, "context-error", "没有缓存的游戏数据".to_string()),
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
        .setup(|app| {
            // 恢复缓存注册表并激活最近使用的上下文。
            let dir = app
                .path()
                .app_data_dir()
                .map(|dir| dir.join("contexts"))
                .unwrap_or_default();
            let state = app.state::<AppState>();
            {
                let mut registry = state.contexts.lock().expect("contexts lock");
                registry.dir = dir;
                registry.scan();
            }
            let newest = state
                .contexts
                .lock()
                .ok()
                .and_then(|registry| {
                    registry
                        .meta
                        .values()
                        .max_by_key(|meta| meta.created_at)
                        .map(|meta| meta.id.clone())
                });
            if let Some(id) = newest {
                let mut runtime = state.runtime.lock().expect("runtime lock");
                if let Err(error) = ensure_context_loaded(&state, &mut runtime, &id) {
                    eprintln!("failed to load cached context: {error}");
                } else {
                    runtime.set_active_context(Some(id));
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_bundled_dump,
            load_game_context,
            load_dump,
            list_contexts,
            set_active_context,
            rename_context,
            delete_context,
            pick_game_executable,
            pick_dump_file,
            pick_mod_dir,
            icon,
            catalog_index,
            prototype_detail,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_recipe_categories_default_to_crafting() {
        let recipe = RecipeComponent {
            categories: None,
            ..Default::default()
        };
        assert_eq!(effective_recipe_categories(&recipe), vec!["crafting"]);
        let recipe = RecipeComponent {
            categories: Some(vec!["smelting".to_string()]),
            ..Default::default()
        };
        assert_eq!(effective_recipe_categories(&recipe), vec!["smelting"]);
    }

    #[test]
    fn empty_resource_category_defaults_to_basic_solid() {
        let resource = ResourceEntityComponent {
            category: String::new(),
            ..Default::default()
        };
        assert_eq!(effective_resource_category(&resource), "basic-solid");
        let resource = ResourceEntityComponent {
            category: "calcite".to_string(),
            ..Default::default()
        };
        assert_eq!(effective_resource_category(&resource), "calcite");
    }
}
