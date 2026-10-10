//! Tauri adapter：把 GUI 无关的 `metatorio-shell` 接到 Tauri 上。
//!
//! 业务逻辑全在 `metatorio-shell`。这一层只负责：
//! - [`TauriHost`]：用 `AppHandle` 实现 shell 的 `Host`（emit / app_data_dir）；
//! - [`run`]：建窗口、装插件、启 MCP、恢复缓存上下文；
//! - 一组 `#[tauri::command]` 薄包装：参数转换后转发给 shell；
//! - 纯 GUI 的文件对话框命令（`pick_*`）。

use std::path::PathBuf;
use std::sync::Arc;

use metatorio_core::{Accessible, DualVar};
use metatorio_runtime::document::AppDocument;
use metatorio_runtime::id::{FactoryId, MechanicId, ProjectId};
use metatorio_runtime::message::AppMessage;
use metatorio_runtime::state::DispatchResult;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use metatorio_shell::app::{
    AppState, CatalogIndex, ContextInfo, ContextList, PrototypeDetail, Suggestion,
};
use metatorio_shell::host::Host;

pub use metatorio_shell::Options;
pub use metatorio_shell::mcp;

/// 用 `AppHandle` 实现 shell 的 `Host`：GUI 副作用（emit / 应用数据目录）在这里落地。
///
/// 与 shell 共享同一个 `Arc<AppState>`——两者生命周期/线程都不保证一致，所以
/// 一律经 `Arc` 共享，不假设谁先谁后。
pub struct TauriHost {
    app: AppHandle,
    state: Arc<AppState>,
}

impl TauriHost {
    fn new(app: AppHandle, state: Arc<AppState>) -> Self {
        Self { app, state }
    }
}

impl Host for TauriHost {
    fn state(&self) -> &AppState {
        &self.state
    }

    fn app_data_dir(&self) -> PathBuf {
        self.app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
    }

    fn emit(&self, event: &str, payload: serde_json::Value) {
        let _ = self.app.emit(event, payload);
    }
}

type HostState<'a> = State<'a, Arc<TauriHost>>;

/// 从 Tauri 管理的 `Arc<TauriHost>` 取一个 `Arc<dyn Host>` 交给 shell。
fn host_of(state: &HostState<'_>) -> Arc<dyn Host> {
    state.inner().clone()
}

// ── 命令薄包装（原逻辑在 metatorio-shell::app）──────────────────────

#[tauri::command]
async fn load_bundled_dump(host: HostState<'_>) -> Result<ContextInfo, String> {
    metatorio_shell::app::load_bundled_dump(&host_of(&host)).await
}

#[tauri::command]
async fn load_game_context(
    host: HostState<'_>,
    executable_path: String,
    mod_dir: Option<String>,
) -> Result<ContextInfo, String> {
    metatorio_shell::app::load_game_context(&host_of(&host), executable_path, mod_dir).await
}

#[tauri::command]
async fn load_dump(host: HostState<'_>, path: String) -> Result<ContextInfo, String> {
    metatorio_shell::app::load_dump(&host_of(&host), path).await
}

#[tauri::command]
fn list_contexts(host: HostState<'_>) -> ContextList {
    let host = host_of(&host);
    metatorio_shell::app::list_contexts(host.state())
}

#[tauri::command]
fn icon(host: HostState<'_>, ty: String, name: String, context_id: String) -> Option<Vec<u8>> {
    let host = host_of(&host);
    metatorio_shell::app::icon(host.state(), ty, name, context_id)
}

#[tauri::command]
async fn catalog_index(host: HostState<'_>, context_id: String) -> Result<CatalogIndex, String> {
    metatorio_shell::app::catalog_index(&host_of(&host), context_id).await
}

#[tauri::command]
async fn implicit_sources(
    host: HostState<'_>,
    project: ProjectId,
    factory: FactoryId,
) -> Result<Vec<DualVar>, String> {
    metatorio_shell::app::implicit_sources(&host_of(&host), project, factory).await
}

#[tauri::command]
async fn suggest(
    host: HostState<'_>,
    context_id: String,
    flow: DualVar,
) -> Result<Vec<Suggestion>, String> {
    metatorio_shell::app::suggest(&host_of(&host), context_id, flow).await
}

#[tauri::command]
async fn mechanic_flow(
    host: HostState<'_>,
    project: ProjectId,
    factory: FactoryId,
    mechanic: MechanicId,
) -> Result<Vec<(DualVar, f64)>, String> {
    metatorio_shell::app::mechanic_flow(&host_of(&host), project, factory, mechanic).await
}

#[tauri::command]
async fn solar_balance(
    host: HostState<'_>,
    project: ProjectId,
    factory: FactoryId,
    mechanic: MechanicId,
) -> Result<Option<metatorio_core::SolarBalance>, String> {
    metatorio_shell::app::solar_balance(&host_of(&host), project, factory, mechanic).await
}

#[tauri::command]
async fn allowed_modules(
    host: HostState<'_>,
    context_id: String,
    machine_kind: String,
    machine: String,
    recipe: Option<String>,
) -> Result<Vec<String>, String> {
    metatorio_shell::app::allowed_modules(
        &host_of(&host),
        context_id,
        machine_kind,
        machine,
        recipe,
    )
    .await
}

#[tauri::command]
fn prototype_detail(
    host: HostState<'_>,
    context_id: String,
    kind: String,
    name: String,
) -> Result<Option<PrototypeDetail>, String> {
    let host = host_of(&host);
    metatorio_shell::app::prototype_detail(host.state(), context_id, kind, name)
}

#[tauri::command]
async fn dispatch(host: HostState<'_>, message: AppMessage) -> Result<DispatchResult, String> {
    metatorio_shell::app::dispatch(&host_of(&host), message).await
}

#[tauri::command]
async fn get_document(host: HostState<'_>) -> Result<AppDocument, String> {
    metatorio_shell::app::get_document(&host_of(&host)).await
}

#[tauri::command]
async fn history_state(host: HostState<'_>) -> Result<metatorio_runtime::HistoryStatus, String> {
    metatorio_shell::app::history_state(&host_of(&host)).await
}

#[tauri::command]
async fn accessibility(host: HostState<'_>, project: ProjectId) -> Result<Vec<Accessible>, String> {
    metatorio_shell::app::accessibility(&host_of(&host), project).await
}

#[tauri::command]
async fn milestones_ordered(
    host: HostState<'_>,
    project: ProjectId,
) -> Result<Vec<metatorio_runtime::Milestone>, String> {
    metatorio_shell::app::milestones_ordered(&host_of(&host), project).await
}

#[tauri::command]
async fn productivity(
    host: HostState<'_>,
    project: ProjectId,
) -> Result<metatorio_runtime::ProductivityView, String> {
    metatorio_shell::app::productivity(&host_of(&host), project).await
}

/// 项目记忆的保存路径（未保存过返回 null），供界面显示"保存位置"。
#[tauri::command]
fn project_save_path(host: HostState<'_>, project: ProjectId) -> Option<String> {
    let host = host_of(&host);
    metatorio_shell::app::project_save_path(host.state(), project)
}

// ── 纯 GUI：文件对话框（留在 Tauri 外壳）────────────────────────────

/// OS file dialog for the Factorio executable (game-context loading).
#[tauri::command]
async fn pick_game_executable(app: AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(windows)]
        let picked = app
            .dialog()
            .file()
            .add_filter("Factorio 可执行文件", &["exe"])
            .blocking_pick_file();
        #[cfg(not(windows))]
        let picked = app.dialog().file().blocking_pick_file();
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

/// OS 文件对话框：选一个工程文件路径。真正的导入走
/// `ApplicationAction::OpenProject` 消息（`RuntimeCommand::LoadProject`），
/// 这样 GUI 与 MCP agent 共用同一条导入路径。
#[tauri::command]
async fn pick_project_file(app: AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let picked = app
            .dialog()
            .file()
            .add_filter("Metatorio 工程", &["json", "fpp"])
            .blocking_pick_file();
        Ok(picked
            .and_then(|picked| picked.into_path().ok())
            .map(|path| path.to_string_lossy().to_string()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// OS 文件对话框：选「另存为」的路径。写盘走
/// `ApplicationAction::SaveProjectAs` 消息（`RuntimeCommand::Persist`）。
#[tauri::command]
async fn pick_project_save_path(app: AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let picked = app
            .dialog()
            .file()
            .set_file_name("metatorio-project.json")
            .add_filter("Metatorio 工程", &["json", "fpp"])
            .blocking_save_file();
        Ok(picked
            .and_then(|picked| picked.into_path().ok())
            .map(|path| path.to_string_lossy().to_string()))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(options: Options) {
    // 无头模式：在 build 之前清空窗口配置——setup 照常跑（注册表扫描、恢复最近
    // 上下文、启动 MCP），但一个窗口都不建。
    let mut context = tauri::generate_context!();
    if options.headless {
        context.config_mut().app.windows.clear();
        if options.mcp {
            println!(
                "切向量化 headless：不创建窗口，MCP 端点在 http://{}:{}{}",
                options.mcp_bind,
                options.mcp_port,
                metatorio_shell::MCP_PATH
            );
        } else {
            println!("切向量化 headless：不创建窗口，且已关闭 MCP（没有任何接口）");
        }
    }
    let mcp_enabled = options.mcp;
    let mcp_config = metatorio_shell::mcp::ServerConfig {
        bind: options.mcp_bind.clone(),
        port: options.mcp_port,
        token: options.mcp_token.clone(),
        allow_hosts: options.mcp_allow_hosts.clone(),
        tools: options.mcp_tools.clone(),
    };
    let builder_options = options.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            let state = Arc::new(AppState::with_options(&builder_options));
            let host: Arc<TauriHost> =
                Arc::new(TauriHost::new(app.handle().clone(), state.clone()));
            app.manage(host.clone());
            let shell_host: Arc<dyn Host> = host.clone();
            // 先把 MCP 端点起起来：恢复缓存上下文可能要读几十 MB 的 dump（真机上
            // 数秒），而 MCP 客户端往往启动后立刻连接。
            #[cfg(not(mobile))]
            if mcp_enabled {
                metatorio_shell::mcp::spawn_server(shell_host.clone(), mcp_config.clone());
            }
            metatorio_shell::app::restore_contexts(&shell_host);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_bundled_dump,
            load_game_context,
            load_dump,
            list_contexts,
            pick_game_executable,
            pick_dump_file,
            pick_mod_dir,
            icon,
            catalog_index,
            prototype_detail,
            suggest,
            implicit_sources,
            mechanic_flow,
            solar_balance,
            allowed_modules,
            dispatch,
            get_document,
            history_state,
            accessibility,
            milestones_ordered,
            productivity,
            pick_project_file,
            pick_project_save_path,
            project_save_path,
        ])
        .run(context)
        .expect("error while running tauri application");
}
