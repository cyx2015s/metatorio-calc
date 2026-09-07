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
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use rmcp::{
    ErrorData as McpError,
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    tool, tool_router,
    transport::streamable_http_server::{
        session::local::LocalSessionManager,
        StreamableHttpServerConfig, StreamableHttpService,
    },
};
use schemars::JsonSchema;
use tauri::{AppHandle, Emitter, Manager};

use crate::{AppState, execute_command};
use metatorio_runtime::message::AppMessage;
use metatorio_runtime::{CommandEffect, FactoryId, ProjectId, RuntimeCommand};

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
    #[tool(description = "Forward one AppMessage to the Metatorio planner runtime \
        (project / factory / mechanism / solve) and return the resulting revision. \
        This is the universal escape hatch for every planning operation; wire \
        convenience tools on top of it as needed.")]
    async fn dispatch(
        &self,
        Parameters(params): Parameters<DispatchParams>,
    ) -> Result<CallToolResult, McpError> {
        let message = params.message;

        let app = self.app.clone();
        let handled = tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<AppState>();
            let mut runtime = state
                .runtime
                .lock()
                .map_err(|_| "runtime lock poisoned".to_string())?;
            let outcome = runtime.dispatch(message).map_err(|error| error.to_string())?;
            // 求解结果结构化为 JSON；命令列表真实返回（而非仅数量）。
            let mut solve: Option<serde_json::Value> = None;
            let mut commands: Vec<serde_json::Value> = Vec::new();
            for command in &outcome.commands {
                if let Some(effect) = execute_command(&app, &state, &mut runtime, command) {
                    if let metatorio_runtime::CommandEffect::Solve(result) = effect {
                        solve = serde_json::to_value(&result).ok();
                    }
                }
                if let Ok(value) = serde_json::to_value(command) {
                    commands.push(value);
                }
            }
            // Co-op: if the document changed, tell the GUI to re-fetch.
            if outcome.changed {
                let _ = app.emit("document-changed", outcome.revision);
            }
            Ok::<_, String>(DispatchHandled {
                revision: outcome.revision,
                changed: outcome.changed,
                commands,
                solve,
            })
        })
        .await
        .map_err(|error| McpError::internal_error(format!("dispatch join 失败: {error}"), None))?;

        let handled = handled.map_err(|error| {
            McpError::invalid_params(format!("dispatch 执行失败: {error}"), None)
        })?;

        let payload = serde_json::json!({
            "revision": handled.revision,
            "changed": handled.changed,
            "scheduled_commands": handled.commands,
            "solve": handled.solve,
        });
        Ok(CallToolResult::structured(payload))
    }

    /// Read the current planning state (the shared document snapshot).  This is
    /// the reading counterpart to `dispatch`: it lets an agent observe projects /
    /// factories / targets / mechanics and their assigned ids before mutating.
    #[tool(description = "Read the current planning state from the shared Metatorio \
        document.  Omit `project` to return the whole document; pass `project` to \
        narrow to one project; pass `project` + `factory` to narrow to one factory. \
        Set `recompute` (only meaningful with project + factory) to also run a solve \
        and include the structured result.")]
    async fn get_planning_state(
        &self,
        Parameters(params): Parameters<PlanningStateParams>,
    ) -> Result<CallToolResult, McpError> {
        let app = self.app.clone();
        let handled = tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<AppState>();
            let mut runtime = state
                .runtime
                .lock()
                .map_err(|_| "runtime lock poisoned".to_string())?;

            let project = params.project.map(ProjectId);
            let factory = params.factory.map(FactoryId);

            // 收集所选层级的文档快照。
            let mut snapshot = match (project, factory) {
                (None, _) => serde_json::to_value(&runtime.state.document).map_err(|e| e.to_string())?,
                (Some(p), None) => {
                    let doc = runtime
                        .state
                        .project(p)
                        .map_err(|e| e.to_string())?;
                    serde_json::to_value(doc).map_err(|e| e.to_string())?
                }
                (Some(p), Some(f)) => {
                    // 求解（可选）：仅当项目上下文已载入且请求时触发。
                    let solve = if params.recompute {
                        let command = RuntimeCommand::Recompute {
                            project: p,
                            factory: f,
                        };
                        match runtime.run_command(&command) {
                            Ok(effect) => match effect {
                                CommandEffect::Solve(result) => {
                                    Some(serde_json::to_value(&result).map_err(|e| e.to_string())?)
                                }
                                _ => None,
                            },
                            Err(error) => {
                                return Err(format!("recompute failed: {error}"));
                            }
                        }
                    } else {
                        None
                    };
                    let doc = runtime
                        .state
                        .factory(p, f)
                        .map_err(|e| e.to_string())?;
                    let factory_doc = serde_json::to_value(doc).map_err(|e| e.to_string())?;
                    serde_json::to_value(serde_json::json!({
                        "project": p.0,
                        "factory": f.0,
                        "factory_document": factory_doc,
                        "solve": solve,
                    }))
                    .map_err(|e| e.to_string())?
                }
            };
            // 顶层补充 revision，便于 agent 得知文档版本。
            if let serde_json::Value::Object(obj) = &mut snapshot {
                obj.insert(
                    "revision".to_string(),
                    serde_json::Value::Number(runtime.state.revision.into()),
                );
            }
            Ok::<_, String>(snapshot)
        })
        .await
        .map_err(|error| McpError::internal_error(format!("get_planning_state join 失败: {error}"), None))?;

        let snapshot = handled.map_err(|error| {
            McpError::invalid_params(format!("get_planning_state 执行失败: {error}"), None)
        })?;

        Ok(CallToolResult::structured(snapshot))
    }
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

struct DispatchHandled {
    revision: u64,
    changed: bool,
    commands: Vec<serde_json::Value>,
    solve: Option<serde_json::Value>,
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
