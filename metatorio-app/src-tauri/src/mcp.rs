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
use metatorio_core::DualVar;
use metatorio_runtime::message::AppMessage;
use metatorio_runtime::{FactoryId, ProjectId};

/// Default loopback port for the MCP endpoint (`--mcp-port` /
/// `METATORIO_MCP_PORT` 可覆盖）。
pub const DEFAULT_MCP_PORT: u16 = 8765;

/// The MCP service routes are mounted under this path (e.g.
/// `http://127.0.0.1:8765/mcp`).
pub const MCP_PATH: &str = "/mcp";

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
    ///
    /// **单位**：所有流量（目标 amount、外部输入、求解结果 flows）都是
    /// **每秒**；项目的 `time-scale`（seconds/minutes/hours）只影响界面显示，
    /// 不改变数值。
    message: AppMessage,
    /// 幂等键（可选）：同一个 id 的重复调用只应用一次，重试直接拿回上次的
    /// 载荷（返回里带 `idempotent_replay: true`）。调用超时后重试请带上它，
    /// 避免重复添加目标 / 机制。
    #[serde(default)]
    request_id: Option<String>,
}

/// Parameters for the `list_prototypes` tool: the domain vocabulary of one
/// game context, optionally narrowed by kind and/or a name substring.
#[derive(Debug, serde::Deserialize, JsonSchema)]
struct ListPrototypesParams {
    /// 要查询的游戏上下文 id；省略 = 当前激活的上下文。
    #[serde(default)]
    context_id: Option<String>,
    /// 按条目 kind 精确过滤，例如 `item` / `fluid` / `recipe` / `technology` /
    /// `machine` / `mining-machine` / `generator` / `boiler` / `reactor` /
    /// `solar-panel` / `accumulator` / `beacon` / `resource` / `module` /
    /// `planet` / `surface`。省略 = 全部 kind。
    #[serde(default)]
    kind: Option<String>,
    /// 名字包含（大小写不敏感子串）；同时匹配 `name` 与 `localized_name`。
    /// 省略 = 不过滤。
    #[serde(default)]
    name_contains: Option<String>,
}

/// Parameters for the `localized_names` tool: prototype ids and/or localized names.
#[derive(Debug, serde::Deserialize, JsonSchema)]
struct LocalizedNamesParams {
    /// 要解析的名字，可混用原型 id（`iron-gear-wheel`）与本地化名（`铁齿轮`）。
    queries: Vec<String>,
    /// 限定条目 kind（`item` / `fluid` / `recipe` / `technology` / `machine` / …）；
    /// 省略 = 全部。
    #[serde(default)]
    kind: Option<String>,
    /// 每个查询的**模糊**命中上限（默认 8，最多 50）；精确命中不受此限制。
    #[serde(default)]
    limit_per_query: Option<usize>,
    /// 游戏上下文 id；省略 = 当前激活上下文。
    #[serde(default)]
    context_id: Option<String>,
}

/// Parameters for the `suggest` tool: candidates that can provide/consume a flow.
#[derive(Debug, serde::Deserialize, JsonSchema)]
struct SuggestParams {
    /// 要查建议的流——与 `dispatch` 里目标/外部输入用的 `DualVar` 同形
    /// （如 `{"Item":{"id":"iron-plate","quality":"normal"}}` 或
    /// `{"Fluid":{"name":"water","temperature":[15,15]}}` 或 `"Electricity"`）。
    flow: DualVar,
    /// 游戏上下文 id；省略 = 当前激活的上下文。
    #[serde(default)]
    context_id: Option<String>,
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
        All flow amounts are per second; the project time-scale only affects display. \
        Pass `request_id` to make retries idempotent (a repeated id replays the \
        previous response instead of applying the message again). \
        This is the universal escape hatch for every planning operation; wire \
        convenience tools on top of it as needed."
    )]
    async fn dispatch(
        &self,
        Parameters(params): Parameters<DispatchParams>,
    ) -> Result<CallToolResult, McpError> {
        dispatch_message(&self.app, params.message, params.request_id).await
    }

    /// Read the current planning state (the shared document snapshot).  This is
    /// the reading counterpart to `dispatch`: it lets an agent observe projects /
    /// factories / targets / mechanics and their assigned ids before mutating.
    #[tool(
        description = "Read the current planning state from the shared Metatorio \
        document.  Omit `project` to return the whole document; pass `project` to \
        narrow to one project; pass `project` + `factory` to narrow to one factory. \
        Set `recompute` (only meaningful with project + factory) to also run a solve \
        and include the structured result.  All flow amounts are per second \
        (time-scale only affects display)."
    )]
    async fn get_planning_state(
        &self,
        Parameters(params): Parameters<PlanningStateParams>,
    ) -> Result<CallToolResult, McpError> {
        let app = self.app.clone();
        let project = params.project.map(ProjectId);
        let factory = params.factory.map(FactoryId);
        // `recompute` 只在 project + factory 同时给出时才有意义：其余组合显式
        // 报错，而不是静默忽略（agent 之前无法察觉）。
        if params.recompute && (project.is_none() || factory.is_none()) {
            return Err(McpError::invalid_params(
                "recompute 需要同时提供 project 与 factory".to_string(),
                None,
            ));
        }

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

    /// 项目索引：有哪些项目、各自多少个工厂 / 机制 / 目标。
    ///
    /// 给 agent 一个**便宜的第一步**：先看索引再决定读哪个项目的完整文档，
    /// 避免为了找一个 id 而拉全量文档。
    #[tool(
        description = "List projects (id, name, context, and counts of factories / \
        mechanics / targets).  Cheap index: call this first, then use \
        get_planning_state to read a specific project."
    )]
    async fn list_projects(&self) -> Result<CallToolResult, McpError> {
        let app = self.app.clone();
        let projects = tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<AppState>();
            let runtime = state
                .runtime
                .lock()
                .map_err(|_| "runtime lock poisoned".to_string())?;
            Ok::<_, String>(project_summary(&runtime.state.document))
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("list_projects join 失败: {error}"), None)
        })?
        .map_err(|error| {
            McpError::invalid_params(format!("list_projects 执行失败: {error}"), None)
        })?;
        Ok(CallToolResult::structured(serde_json::json!({
            "projects": projects,
        })))
    }

    /// 工厂索引：某个项目下有哪些工厂、规模与关键设置（含目标清单）。
    #[tool(
        description = "List the factories of one project (id, name, planet/surface, \
        major quality, strict source/sink, counts, and the target list).  Cheap \
        index: call this first, then get_planning_state with project + factory to \
        read the full factory document."
    )]
    async fn list_factories(
        &self,
        Parameters(params): Parameters<ListFactoriesParams>,
    ) -> Result<CallToolResult, McpError> {
        let app = self.app.clone();
        let project = ProjectId(params.project);
        let (name, factories) = tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<AppState>();
            let runtime = state
                .runtime
                .lock()
                .map_err(|_| "runtime lock poisoned".to_string())?;
            let project_doc = runtime
                .state
                .project(project)
                .map_err(|error| error.to_string())?;
            Ok::<_, String>((project_doc.name.clone(), factory_summary(project_doc)))
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("list_factories join 失败: {error}"), None)
        })?
        .map_err(|error| {
            McpError::invalid_params(format!("list_factories 执行失败: {error}"), None)
        })?;
        Ok(CallToolResult::structured(serde_json::json!({
            "project": params.project,
            "name": name,
            "factories": factories,
        })))
    }

    /// 上下文索引：有哪些游戏数据上下文（dump/导出缓存）、激活的是哪个。
    ///
    /// agent 需要它才能理解 `project.context_id` 的含义，并在多个上下文之间
    /// 切换（`dispatch` + `{"scope":"application","action":{"set-active-context":…}}`）。
    #[tool(
        description = "List the registered game-data contexts (id, display name, source, \
        whether its prototype store is currently loaded, and which one is active). \
        A project's `context_id` points at one of these ids; switch with dispatch \
        {scope: application, action: {set-active-context: {context: id}}}. \
        Contexts are content-hashed caches of exported game data — an agent cannot \
        create one, only list / activate / rename / delete existing ones."
    )]
    async fn list_contexts(&self) -> Result<CallToolResult, McpError> {
        let app = self.app.clone();
        let list = tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<AppState>();
            crate::context_list(&state)
        })
        .await
        .map_err(|error| {
            McpError::internal_error(format!("list_contexts join 失败: {error}"), None)
        })?;
        let value = serde_json::to_value(&list).map_err(|error| {
            McpError::internal_error(format!("list_contexts 序列化失败: {error}"), None)
        })?;
        Ok(CallToolResult::structured(value))
    }

    /// 领域词表：某个游戏上下文里有哪些原型（物品/流体/配方/科技/机器/…）。
    ///
    /// 消除 agent 的「盲猜字符串」：dispatch 里的 recipe/machine/item/fluid 名字
    /// 必须真实存在（校验会拒绝不存在的名字），这里给出合法取值。
    #[tool(
        description = "List the prototypes of a game context (the domain vocabulary): \
        items / fluids / recipes / technologies / machines / resources / qualities … \
        with name, localized_name, group/subgroup, categories, fuel info and module \
        slots.  Omit `context_id` to use the active context.  Narrow with `kind` \
        (exact) and/or `name_contains` (case-insensitive substring on name or \
        localized_name).  Returns `total` (all entries in the context), `matched` and \
        the filtered `entries` — no truncation, so filter rather than paginate.  \
        For 'id → localized name' or 'a name someone said in chat → id' use \
        `localized_names` instead: it is ranked (exact hit first) and answers several \
        names at once."
    )]
    async fn list_prototypes(
        &self,
        Parameters(params): Parameters<ListPrototypesParams>,
    ) -> Result<CallToolResult, McpError> {
        let kind = params.kind;
        let needle = params.name_contains;
        let state = self.app.state::<AppState>();
        // 只是读一次 runtime 里的激活上下文 id（短锁），无需阻塞线程。
        let context_id = crate::resolve_context_id(&state, params.context_id.as_deref())
            .map_err(|error| McpError::invalid_params(error, None))?;
        let index = crate::catalog_index_for(&state, &context_id)
            .await
            .map_err(|error| {
                McpError::invalid_params(format!("list_prototypes 执行失败: {error}"), None)
            })?;
        let total = index.entries.len();
        let entries =
            crate::filter_index_entries(index.entries, kind.as_deref(), needle.as_deref());
        let matched = entries.len();
        let value = serde_json::json!({
            "context_id": context_id,
            "qualities": index.qualities,
            "total": total,
            "matched": matched,
            "entries": entries,
        });
        Ok(CallToolResult::structured(value))
    }

    /// 名字 ↔ 本地化名互查：把求解结果里的 id 换成人话，或把群友口述的名字换成 id。
    #[tool(
        description = "Resolve prototype names to their localized (translated) names and \
        back.  Pass `queries` with either a raw prototype id (e.g. `iron-gear-wheel`) or a \
        localized name as a player would say it (e.g. `铁齿轮`); each query is matched \
        exactly first (raw id, then localized name), then by prefix/substring, and every \
        hit reports `matched_by` so you can tell an exact hit from a loose one.  \
        Separators are ignored while matching: `processing unit`, `processing_unit` and \
        `PROCESSING-UNIT` all hit `processing-unit` (returned `name` keeps the real id).  \
        Use this to (a) report solve results in the player's language instead of raw ids \
        and (b) turn an item name someone mentioned in chat back into the id that \
        `dispatch` needs.  \
        When nothing matches exactly or partially, the `typo` bucket holds \
        typo-tolerant candidates for queries of 3+ characters, each with an edit \
        `distance` (adjacent transpositions count as 1).  `typo_suggestion` is the \
        high-confidence pick and is non-null **only when a single name is uniquely \
        closest** (the same name in several prototype groups is not ambiguous — pick \
        the `kind` you need from `typo`); when it is null, several names are equally \
        close (`processing-unit-2` vs `-3`) or none is close enough — ask the human \
        instead of guessing, because a wrong prototype id validates fine and silently \
        produces the wrong plan.  \
        `localized_name` is empty when the context has no locale dump (then fall back to \
        `list_prototypes`).  `exact` may contain several entries for one query: the same \
        name can exist as item / recipe / technology / entity, and `kind` narrows it."
    )]
    async fn localized_names(
        &self,
        Parameters(params): Parameters<LocalizedNamesParams>,
    ) -> Result<CallToolResult, McpError> {
        let state = self.app.state::<AppState>();
        let context_id = crate::resolve_context_id(&state, params.context_id.as_deref())
            .map_err(|error| McpError::invalid_params(error, None))?;
        let index = crate::catalog_index_for(&state, &context_id)
            .await
            .map_err(|error| {
                McpError::invalid_params(format!("localized_names 执行失败: {error}"), None)
            })?;
        // kind 过滤只做一次（整个查询共用同一份索引子集）。
        let entries = crate::filter_index_entries(index.entries, params.kind.as_deref(), None);
        let limit = params.limit_per_query.unwrap_or(8).clamp(1, 50);
        let results: Vec<serde_json::Value> = params
            .queries
            .iter()
            .map(|query| {
                let resolved = crate::resolve_index_entry(&entries, query, limit);
                serde_json::json!({
                    "query": query,
                    // 精确 + 模糊命中的总数（模糊部分在截断前计数；typo 单独统计）。
                    "matched": resolved.exact.len() + resolved.partial_matched,
                    // 模糊结果被 limit 截断时置 true：要更全就调大 limit_per_query，
                    // 或用 `list_prototypes` 的 name_contains 自己筛。
                    "partial_truncated": resolved.partial_matched > resolved.partial.len(),
                    "exact": resolved.exact,
                    "partial": resolved.partial,
                    // 错拼候选：只在精确与模糊都为空时才有内容。typo_suggestion 是
                    // 「高置信度结果」——只有最佳距离上**名字唯一**时才非空（同名跨
                    // item/recipe 不算歧义）；为 null 说明有几个同样接近的名字
                    // （processing-unit-2 与 -3）或都没凑到，需要问人。
                    "typo": resolved.typo,
                    "typo_matched": resolved.typo_matched,
                    "typo_best_distance": resolved.typo_best_distance,
                    "typo_best_name_count": resolved.typo_best_name_count,
                    "typo_suggestion": resolved.typo_suggestion,
                })
            })
            .collect();
        Ok(CallToolResult::structured(serde_json::json!({
            "context_id": context_id,
            "kind": params.kind,
            "results": results,
        })))
    }

    /// 建议：给定一条流，列出能产出/消耗它的候选机制。
    #[tool(
        description = "Suggest mechanics that could provide or consume one flow in the \
        active game context (recipes, resource patches, fuels, generators), each as \
        {kind, name, role} where role='producer' produces the flow and \
        role='consumer' consumes it.  This is the cheap first step before adding a \
        mechanic: pick a candidate, then dispatch a mechanic-list add + the matching \
        set-recipe / set-resource / set-item / set-generator message."
    )]
    async fn suggest(
        &self,
        Parameters(params): Parameters<SuggestParams>,
    ) -> Result<CallToolResult, McpError> {
        let flow = params.flow.clone();
        let state = self.app.state::<AppState>();
        let context_id = crate::resolve_context_id(&state, params.context_id.as_deref())
            .map_err(|error| McpError::invalid_params(error, None))?;
        let suggestions = crate::suggest_for(&state, &context_id, flow.clone())
            .await
            .map_err(|error| {
                McpError::invalid_params(format!("suggest 执行失败: {error}"), None)
            })?;
        Ok(CallToolResult::structured(serde_json::json!({
            "context_id": context_id,
            "flow": flow,
            "suggestions": suggestions,
        })))
    }
}

/// `dispatch` 工具的实际逻辑：与具体 Tauri runtime 解耦，便于用 mock app 测试。
///
/// `request_id` 为幂等键：重复的 id 不重新应用消息，直接回放上次的载荷。
async fn dispatch_message<R: Runtime>(
    app: &AppHandle<R>,
    message: AppMessage,
    request_id: Option<String>,
) -> Result<CallToolResult, McpError> {
    // 0) 幂等回放：同一 request_id 已经执行过就直接返回上次的载荷。
    if let Some(request_id) = &request_id {
        let cached = app
            .state::<AppState>()
            .dispatch_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(request_id));
        if let Some((mut payload, is_error)) = cached {
            if let serde_json::Value::Object(object) = &mut payload {
                object.insert(
                    "idempotent_replay".to_string(),
                    serde_json::Value::Bool(true),
                );
            }
            let mut result = CallToolResult::structured(payload);
            if is_error {
                result.is_error = Some(true);
            }
            return Ok(result);
        }
    }

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
    let is_error = !errors.is_empty();
    // 只记录**成功**的幂等结果：失败（尤其求解超时）必须允许用同一个 id
    // 重试，否则重试会一直回放失败。
    if !is_error {
        if let Some(request_id) = request_id {
            if let Ok(mut cache) = app.state::<AppState>().dispatch_cache.lock() {
                cache.insert(request_id, payload.clone(), false);
            }
        }
    }
    let mut result = CallToolResult::structured(payload);
    if is_error {
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

/// Parameters for `list_factories`.
#[derive(Debug, serde::Deserialize, JsonSchema)]
struct ListFactoriesParams {
    /// Project id (u64).
    project: u64,
}

/// 项目级索引条目：只给「有哪些项目、各自多大」，不给内容。
fn project_summary(document: &metatorio_runtime::AppDocument) -> Vec<serde_json::Value> {
    document
        .projects
        .iter()
        .map(|project| {
            serde_json::json!({
                "id": project.id.0,
                "name": project.name,
                "context_id": project.context_id,
                "factories": project.factories.len(),
                "mechanics": project
                    .factories
                    .iter()
                    .map(|factory| factory.mechanics.len())
                    .sum::<usize>(),
                "targets": project
                    .factories
                    .iter()
                    .map(|factory| factory.targets.len())
                    .sum::<usize>(),
            })
        })
        .collect()
}

/// 工厂级索引条目：规模与关键设置，供 agent 决定去读哪一个工厂。
fn factory_summary(project: &metatorio_runtime::ProjectDocument) -> Vec<serde_json::Value> {
    project
        .factories
        .iter()
        .map(|factory| {
            serde_json::json!({
                "id": factory.id.0,
                "name": factory.name,
                "planet": factory.settings.planet,
                "surface": factory.settings.surface,
                "major_quality": factory.settings.major_quality,
                "strict_source": factory.strict_source,
                "strict_sink": factory.strict_sink,
                "mechanics": factory.mechanics.len(),
                "targets": factory.targets.iter().map(|target| serde_json::json!({
                    "id": target.id.0,
                    "flow": target.flow,
                    "amount": target.amount,
                })).collect::<Vec<_>>(),
                "external_inputs": factory.external_inputs.len(),
                "target_expressions": factory.target_expressions.len(),
            })
        })
        .collect()
}

// ── Server lifecycle ───────────────────────────────────────────────

/// Start the MCP server on a dedicated tokio runtime thread, bound to
/// `127.0.0.1:<port>`.  Fire-and-forget: the thread ends when the app exits.
///
/// `port` / `token` 由启动选项（CLI 或环境变量）解析后传入——这里不再自己读
/// 环境变量，避免出现「CLI 指定了但服务仍按 env 起」的双份真相。
pub fn spawn_server(app: AppHandle, port: u16, token: Option<String>) {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build MCP tokio runtime");
        runtime.block_on(serve(app, port, token));
    });
}

/// Build the axum router + listener and serve MCP until the process exits.
async fn serve(app: AppHandle, port: u16, token: Option<String>) {
    let token = token.filter(|token| !token.is_empty());

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

#[cfg(test)]
mod tests {
    use super::*;
    use metatorio_core::{DualVar, IdWithQuality};
    use metatorio_runtime::document::{AppDocument, FactoryDocument, FlowTarget, ProjectDocument};
    use metatorio_runtime::id::{FactoryId, ProjectId, TargetId};

    /// 索引工具必须「小而有信息」：只给规模与关键字段，不含机制明细。
    #[test]
    fn summaries_are_compact_and_informative() {
        let mut factory = FactoryDocument {
            id: FactoryId(2),
            name: "f".to_string(),
            ..Default::default()
        };
        factory.targets.push(FlowTarget {
            id: TargetId(3),
            flow: DualVar::Item(IdWithQuality::new("iron-plate", "normal")),
            amount: 60.0,
        });
        let mut project = ProjectDocument {
            id: ProjectId(1),
            name: "p".to_string(),
            ..Default::default()
        };
        project.factories.push(factory);
        let document = AppDocument {
            schema_version: metatorio_runtime::DOCUMENT_SCHEMA_VERSION,
            projects: vec![project.clone()],
        };

        let projects = project_summary(&document);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["id"], 1);
        assert_eq!(projects[0]["factories"], 1);
        assert_eq!(projects[0]["targets"], 1);
        assert!(projects[0].get("factories_document").is_none());

        let factories = factory_summary(&project);
        assert_eq!(factories.len(), 1);
        assert_eq!(factories[0]["id"], 2);
        assert_eq!(factories[0]["targets"][0]["amount"], 60.0);
        assert_eq!(
            factories[0]["targets"][0]["flow"]["Item"]["id"],
            "iron-plate"
        );
        assert!(factories[0].get("mechanics").is_some());
    }
}
