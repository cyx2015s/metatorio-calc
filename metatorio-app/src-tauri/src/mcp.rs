//! Model Context Protocol server, merged into the main binary.
//!
//! The server is a localhost Streamable-HTTP endpoint (decisions in
//! `docs/mcp-design.md`): it shares the single managed [`AppState`] (and thus
//! the one [`Runtime`]) with the GUI, so an external agent and a human user
//! operate the same projects in one process.  Every planning operation is a
//! `Runtime::dispatch(AppMessage)` — the MCP surface is therefore a thin,
//! framework-independent wrapper over the same reducer the UI uses.
//!
//! # MVP
//!
//! The minimal viable surface is a single `dispatch` tool that accepts a raw
//! `AppMessage` JSON value and forwards it to the runtime.  This is a
//! deliberate **escape hatch**: it covers the entire message set (project /
//! factory / mechanism / solve) with no per-operation parameter structs, so no
//! schema work is required up front and it never goes stale.  Once agent
//! usage is observed, common operations can be re-wrapped as friendlier
//! dedicated tools on top of the same `dispatch` path.
//!
//! # Security
//!
//! - Bound to `127.0.0.1` only.
//! - The rmcp `StreamableHttpServerConfig` additionally restricts the accepted
//!   `Host` header to loopback names (DNS-rebinding protection).
//! - Optional bearer-token auth: if `METATORIO_MCP_TOKEN` is set, every request
//!   must carry `Authorization: Bearer <token>` (or the raw token); if it is
//!   unset, no auth is required (loopback-only is the fallback).

use std::net::SocketAddr;

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};
use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    tool, tool_router,
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ErrorData as McpError,
};
use schemars::JsonSchema;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::{execute_command, AppState};
use metatorio_runtime::message::AppMessage;
use metatorio_runtime::{FactoryId, ProjectId};

/// Default loopback port for the MCP endpoint (override with `METATORIO_MCP_PORT`).
const DEFAULT_PORT: u16 = 8765;

/// The MCP service routes are mounted under this path (e.g.
/// `http://127.0.0.1:8765/mcp`).
const MCP_PATH: &str = "/mcp";

// ── Tool surface ───────────────────────────────────────────────────

/// Parameters for the `dispatch` escape-hatch tool: a raw `AppMessage` JSON.
#[derive(Debug, serde::Deserialize, JsonSchema)]
struct DispatchParams {
    /// A serialized `AppMessage` (adjacently tagged: `scope` selects the
    /// variant, everything else — including `project` / `factory` — lives
    /// inside `action`):
    ///
    /// - `{ "scope": "application", "action": { "new-project": { "name": "…" } } }`
    /// - `{ "scope": "project", "action": { "project": 1, "action": { "add-factory": { "name": "…", "template": "empty" } } } }`
    /// - `{ "scope": "factory", "action": { "project": 1, "factory": 2, "action": { "flow": { "add-to-target": { "flow": { "Item": { "id": "iron-plate", "quality": "normal" } }, "amount": 60.0 } } } } }`
    ///
    /// The `action` is the same value the UI sends over IPC; see
    /// `metatorio-runtime`'s `AppMessage` for the full set.  On any change the
    /// GUI is refreshed via a `document-changed` broadcast event.
    message: AppMessage,
}

/// The MCP server handler.  Stateless: it only carries the [`AppHandle`] it
/// needs to reach the shared [`AppState`], so rmcp can construct a fresh one
/// per request.
#[derive(Clone)]
pub struct MetatorioMcp {
    app: AppHandle,
}

#[tool_router(server_handler)]
impl MetatorioMcp {
    /// Forward one `AppMessage` to the planner runtime (project / factory /
    /// mechanism / solve), exactly as the GUI's dispatch does, and return the
    /// resulting revision + solve status.  This is the universal escape hatch
    /// for every planning operation.
    #[tool(
        description = "Forward one AppMessage to the Metatorio planner runtime \
        (project / factory / mechanism / solve) and return the resulting revision. \
        The response includes `created` (ids of objects this call created, so no \
        follow-up read is needed), `solve` (structured solve result when the command \
        solves), and `errors` (non-empty + isError when a command failed). \
        This is the universal escape hatch for every planning operation; wire \
        convenience tools on top of it as needed."
    )]
    async fn dispatch(
        &self,
        Parameters(params): Parameters<DispatchParams>,
    ) -> Result<CallToolResult, McpError> {
        dispatch_message(&self.app, params.message).await
    }

    /// Read the current planning state (the shared document snapshot).  This is
    /// the reading counterpart to `dispatch`: it lets an agent observe projects /
    /// factories / targets / mechanics and their assigned ids before mutating.
    #[tool(
        description = "Read the current planning state from the shared Metatorio \
        document.  Omit `project` to return the whole document; pass `project` to \
        narrow to one project; pass `project` + `factory` to narrow to one factory. \
        Set `recompute` (only meaningful with project + factory) to also run a solve \
        and include the structured result."
    )]
    async fn get_planning_state(
        &self,
        Parameters(params): Parameters<PlanningStateParams>,
    ) -> Result<CallToolResult, McpError> {
        let app = self.app.clone();
        let project = params.project.map(ProjectId);
        let factory = params.factory.map(FactoryId);

        // 1) 可选重算：与 GUI 走同一条锁外求解路径（不占 runtime 锁，可与
        //    GUI / 其它 MCP 调用并行）。
        let solve = match (project, factory, params.recompute) {
            (Some(project), Some(factory), true) => {
                let state = app.state::<AppState>();
                match crate::solve_factory_offlock(&app, &state, project, factory).await {
                    Ok(result) => Some(serde_json::to_value(&result).map_err(|error| {
                        McpError::internal_error(format!("solve 序列化失败: {error}"), None)
                    })?),
                    Err(error) => {
                        return Err(McpError::invalid_params(
                            format!("recompute failed: {error}"),
                            None,
                        ))
                    }
                }
            }
            _ => None,
        };

        // 2) 取文档快照（短锁，只克隆）+ 锁外序列化。
        let (snapshot, revision) = {
            let state = app.state::<AppState>();
            let runtime = state
                .runtime
                .lock()
                .map_err(|_| McpError::internal_error("runtime lock poisoned".to_string(), None))?;
            let revision = runtime.state.revision;
            let snapshot = match (project, factory) {
                (None, _) => DocSnapshot::Document(runtime.state.document.clone()),
                (Some(p), None) => DocSnapshot::Project(
                    runtime
                        .state
                        .project(p)
                        .map_err(|error| {
                            McpError::invalid_params(
                                format!("get_planning_state 执行失败: {error}"),
                                None,
                            )
                        })?
                        .clone(),
                ),
                (Some(p), Some(f)) => {
                    let factory_doc = runtime.state.factory(p, f).map_err(|error| {
                        McpError::invalid_params(
                            format!("get_planning_state 执行失败: {error}"),
                            None,
                        )
                    })?;
                    DocSnapshot::Factory {
                        project: p.0,
                        factory: f.0,
                        factory_document: factory_doc.clone(),
                    }
                }
            };
            (snapshot, revision)
        };

        let mut value = match snapshot {
            DocSnapshot::Document(document) => serde_json::to_value(&document),
            DocSnapshot::Project(project) => serde_json::to_value(&project),
            DocSnapshot::Factory {
                project,
                factory,
                factory_document,
            } => serde_json::to_value(serde_json::json!({
                "project": project,
                "factory": factory,
                "factory_document": factory_document,
                "solve": solve,
            })),
        }
        .map_err(|error| {
            McpError::internal_error(format!("get_planning_state 序列化失败: {error}"), None)
        })?;
        // 顶层补充 revision，便于 agent 得知文档版本。
        if let serde_json::Value::Object(obj) = &mut value {
            obj.insert(
                "revision".to_string(),
                serde_json::Value::Number(revision.into()),
            );
        }
        Ok(CallToolResult::structured(value))
    }
}

/// `dispatch` 工具的实际逻辑：与具体 Tauri runtime 解耦，便于用 mock app 测试。
async fn dispatch_message<R: Runtime>(
    app: &AppHandle<R>,
    message: AppMessage,
) -> Result<CallToolResult, McpError> {
    // 1) reducer：短临界区（放进阻塞线程池，避免占用 tokio worker）。
    let reduce_app = app.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let state = reduce_app.state::<AppState>();
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| "runtime lock poisoned".to_string())?;
        runtime.dispatch(message).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| McpError::internal_error(format!("dispatch join 失败: {error}"), None))?
    .map_err(|error| McpError::invalid_params(format!("dispatch 执行失败: {error}"), None))?;

    // 2) 副作用：求解在锁外跑（不阻塞 GUI 与其它 MCP 调用）。
    let state = app.state::<AppState>();
    let state_ref = &state;
    let (solve, commands, errors) = crate::run_commands(&outcome.commands, move |command| {
        // 闭包返回值不能借用参数，故克隆命令进 async 块。
        let command = command.clone();
        async move { execute_command(app, state_ref, &command).await }
    })
    .await;
    let solve = solve.and_then(|result| serde_json::to_value(&result).ok());
    // Co-op: if the document changed, tell the GUI to re-fetch.
    // 命令执行本身也可能改文档（如自动规划回写机制），因此用当前 revision
    // 判定，而不只看 reducer 的 `changed`。
    let revision = {
        let state = app.state::<AppState>();
        state
            .runtime
            .lock()
            .map(|runtime| runtime.state.revision)
            .unwrap_or(outcome.revision)
    };
    if outcome.changed || revision != outcome.revision {
        let _ = app.emit("document-changed", revision);
    }

    let payload = serde_json::json!({
        "revision": revision,
        "changed": outcome.changed || revision != outcome.revision,
        "created": &outcome.created,
        "scheduled_commands": commands,
        "solve": solve,
        "errors": errors,
    });
    // 有失败时把结果标记为错误：agent 必须能区分「命令跑了但失败了」
    // 与「命令跑了且成功但恰好没有求解产出」。
    let mut result = CallToolResult::structured(payload);
    if !errors.is_empty() {
        result.is_error = Some(true);
    }
    Ok(result)
}

/// `get_planning_state` 的读取层级（锁内只克隆，序列化在锁外）。
enum DocSnapshot {
    Document(metatorio_runtime::AppDocument),
    Project(metatorio_runtime::ProjectDocument),
    Factory {
        project: u64,
        factory: u64,
        factory_document: metatorio_runtime::FactoryDocument,
    },
}

/// Parameters for `get_planning_state` (all optional; omit for the whole document).
#[derive(Debug, serde::Deserialize, JsonSchema)]
struct PlanningStateParams {
    /// Project id (u64). Omit to return all projects.
    #[serde(default)]
    project: Option<u64>,
    /// Factory id (u64). Requires `project`.
    #[serde(default)]
    factory: Option<u64>,
    /// When set with `project` + `factory`, run a solve and include its result.
    #[serde(default)]
    recompute: bool,
}

// ── Server lifecycle ───────────────────────────────────────────────

/// Start the MCP server on a dedicated tokio runtime thread, bound to
/// `127.0.0.1:<port>`.  Fire-and-forget: the thread ends when the app exits.
pub fn spawn_server(app: AppHandle) {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build MCP tokio runtime");
        runtime.block_on(serve(app));
    });
}

/// Build the axum router + listener and serve MCP until the process exits.
async fn serve(app: AppHandle) {
    let token = std::env::var("METATORIO_MCP_TOKEN")
        .ok()
        .filter(|token| !token.is_empty());
    let port = std::env::var("METATORIO_MCP_PORT")
        .ok()
        .and_then(|port| port.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);

    let service = StreamableHttpService::new(
        move || Ok(MetatorioMcp { app: app.clone() }),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            // Stateless for every protocol version: each request gets a fresh
            // handler, shared state lives in the managed `AppState`.
            .with_legacy_session_mode(false)
            // Simple request/response tools reply as `application/json`.
            .with_json_response(true),
    );

    let router = Router::new()
        .nest_service(MCP_PATH, service)
        .layer(middleware::from_fn_with_state(token.clone(), require_token));

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("metatorio MCP server failed to bind {addr}: {error}");
            return;
        }
    };
    eprintln!(
        "metatorio MCP server listening on http://{addr}{MCP_PATH}{}",
        if token.is_some() {
            " (token auth enabled)"
        } else {
            " (no token auth; loopback only)"
        }
    );
    if let Err(error) = axum::serve(listener, router).await {
        eprintln!("metatorio MCP server error: {error}");
    }
}

/// Bearer-token gate.  When `token` is `None` (env var unset) this is a no-op;
/// otherwise the request must present `Authorization: Bearer <token>` (or the
/// raw token) to pass.
async fn require_token(
    State(token): State<Option<String>>,
    request: Request,
    next: Next,
) -> Response {
    if let Some(token) = token {
        let authorized = request
            .headers()
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .map(|header| header.strip_prefix("Bearer ").unwrap_or(header))
            .is_some_and(|presented| presented == token);
        if !authorized {
            return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
        }
    }
    next.run(request).await
}
