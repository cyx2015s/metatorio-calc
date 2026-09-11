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
    /// 每个集合的返回上限与偏移（默认 50、上限 1000）——求解结果里的
    /// `mechanics` / `flows` 可能是几百条，必须由工具自己兜住。
    #[serde(flatten)]
    page: PageParams,
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
    /// 每个集合的返回上限与偏移（默认 50、上限 1000）。
    #[serde(flatten)]
    page: PageParams,
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
        `solve` is always bounded: its `mechanics`/`flows` are capped by `limit` \
        (default 50, max 1000) with `offset` for paging, and `page.totals` / \
        `page.truncated` report what was cut. \
        This is the universal escape hatch for every planning operation; wire \
        convenience tools on top of it as needed."
    )]
    async fn dispatch(
        &self,
        Parameters(params): Parameters<DispatchParams>,
    ) -> Result<CallToolResult, McpError> {
        dispatch_message(
            &self.app,
            params.message,
            params.request_id,
            params.page.resolve(),
        )
        .await
    }

    /// Read the current planning state (the shared document snapshot).  This is
    /// the reading counterpart to `dispatch`: it lets an agent observe projects /
    /// factories / targets / mechanics and their assigned ids before mutating.
    #[tool(
        description = "Read the current planning state from the shared Metatorio \
        document, **level by level** so a single call can never flood the context: \
        without `project` you get the project index (ids, names, counts); with \
        `project` you get its settings/planning plus the factory index; with \
        `project`+`factory` you get that factory's document.  Every repeated \
        collection is capped by `limit` (default 50, max 1000) and `offset` pages \
        through it; the response always carries `page.totals` (pre-truncation counts) \
        and `page.truncated` (which collections were cut), so nothing is silently \
        dropped.  Set `recompute` (only with project + factory) to also run a solve \
        and include its result (its mechanics/flows are paged the same way).  All flow \
        amounts are per second (time-scale only affects display)."
    )]
    async fn get_planning_state(
        &self,
        Parameters(params): Parameters<PlanningStateParams>,
    ) -> Result<CallToolResult, McpError> {
        let app = self.app.clone();
        let page = params.page.resolve();
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

        let (mut value, mut report) = planning_state_value(&snapshot, page);
        // 工厂层可选带上重算结果：它的 mechanics/flows 同样按 page 截断。
        if let Some(solve) = solve {
            if let Some(object) = value.as_object_mut() {
                let mut solve = solve;
                if let Some(status) = solve.get_mut("status").and_then(|s| s.as_object_mut()) {
                    if let Some(solved) = status.get_mut("solved").and_then(|s| s.as_object_mut()) {
                        report.page_key(solved, "mechanics", "solve.mechanics", page);
                        report.page_key(solved, "flows", "solve.flows", page);
                    }
                }
                object.insert("solve".to_string(), solve);
            }
        }
        // 顶层补充 revision 与分页元信息：版本便于判断新鲜度，page 说明截断情况。
        if let serde_json::Value::Object(object) = &mut value {
            object.insert(
                "revision".to_string(),
                serde_json::Value::Number(revision.into()),
            );
            object.insert("page".to_string(), report.value(page));
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
        localized_name; separators `-`/`_`/space are ignored).  \
        **The result is always bounded**: `entries` is capped by `limit` (default 50, \
        max 1000) and `offset` pages through the matches; `total` is every entry in the \
        context, `matched` is how many matched before paging, and `page.truncated` tells \
        you whether the list was cut (a `entries_hint` string appears when it was). \
        For 'id → localized name' or 'a name someone said in chat → id' prefer \
        `localized_names`: it is ranked (exact hit first) and answers several names at \
        once."
    )]
    async fn list_prototypes(
        &self,
        Parameters(params): Parameters<ListPrototypesParams>,
    ) -> Result<CallToolResult, McpError> {
        let kind = params.kind;
        let needle = params.name_contains;
        let page = params.page.resolve();
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
        let matched =
            crate::filter_index_entries(index.entries, kind.as_deref(), needle.as_deref());
        let matched_total = matched.len();
        // 分页在这里做（并在 page 元信息里如实上报），调用方不需要替我们兜底。
        let (entries, _) = page.slice(&matched);
        let mut report = PageReport::default();
        report.record("entries", matched_total, entries.len());
        let mut value = serde_json::json!({
            "context_id": context_id,
            "qualities": index.qualities,
            "total": total,
            "matched": matched_total,
            "returned": entries.len(),
            "entries": entries,
            "page": report.value(page),
        });
        if entries.len() < matched_total {
            // 与嵌套集合同一措辞：被截断就说清怎么收窄，别让调用方以为「就这么多」。
            value["entries_hint"] =
                serde_json::json!(truncation_hint(matched_total, entries.len()));
        }
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
        name can exist as item / recipe / technology / entity, and `kind` narrows it.  \
        Each query's buckets are capped by `limit_per_query` (default 8, max 50) and at \
        most 50 queries are accepted per call, so the response stays bounded."
    )]
    async fn localized_names(
        &self,
        Parameters(params): Parameters<LocalizedNamesParams>,
    ) -> Result<CallToolResult, McpError> {
        // 每个查询的输出已经按 limit_per_query 限制；这里再限制一次「一次问多少
        // 个名字」，避免批量调用本身变成上下文炸弹（超过就显式报错，不静默丢）。
        const MAX_QUERIES: usize = 50;
        if params.queries.is_empty() {
            return Err(McpError::invalid_params(
                "queries 不能为空".to_string(),
                None,
            ));
        }
        if params.queries.len() > MAX_QUERIES {
            return Err(McpError::invalid_params(
                format!(
                    "一次最多解析 {MAX_QUERIES} 个名字（收到 {}），请分批调用",
                    params.queries.len()
                ),
                None,
            ));
        }
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
/// `page` 决定返回里 `solve` 的 mechanics/flows 上限（求解可能产出几百条，
/// 实测 py 上一次自动规划就有 757 条机制）。
async fn dispatch_message<R: Runtime>(
    app: &AppHandle<R>,
    message: AppMessage,
    request_id: Option<String>,
    page: Page,
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
    // 求解结果可能很长（机制/流各几百条）：同样按 page 截断并如实上报。
    let mut report = PageReport::default();
    let solve = solve.map(|mut solve| {
        if let Some(status) = solve.get_mut("status").and_then(|s| s.as_object_mut()) {
            if let Some(solved) = status.get_mut("solved").and_then(|s| s.as_object_mut()) {
                report.page_key(solved, "mechanics", "solve.mechanics", page);
                report.page_key(solved, "flows", "solve.flows", page);
            }
        }
        solve
    });
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
        "page": report.value(page),
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

/// 分页参数：所有返回集合的工具共用（`limit` 默认 50、上限 1000）。
///
/// 群里的实测反馈：以前「截断」是调用方（bash/客户端）替我们兜的——一条查询就能让
/// 上下文爆炸。现在**工具自己保证有上界**，并在 `page` 元信息里如实说明哪些集合被
/// 截断了、截断前有多少条，调用方据此翻页或收窄查询。
#[derive(Debug, Clone, Copy, Default, serde::Deserialize, JsonSchema)]
struct PageParams {
    /// 每个集合最多返回多少条（默认 50，上限 1000）。
    #[serde(default)]
    limit: Option<usize>,
    /// 从第几条开始返回（配合 `limit` 翻页）。
    #[serde(default)]
    offset: Option<usize>,
}

impl PageParams {
    const DEFAULT_LIMIT: usize = 50;
    const MAX_LIMIT: usize = 1000;

    fn resolve(&self) -> Page {
        Page {
            offset: self.offset.unwrap_or(0),
            limit: self
                .limit
                .unwrap_or(Self::DEFAULT_LIMIT)
                .clamp(1, Self::MAX_LIMIT),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Page {
    offset: usize,
    limit: usize,
}

impl Page {
    /// 按页切一个数组，返回 `(页内元素, 截断前总数)`。
    fn slice<T: Clone>(&self, items: &[T]) -> (Vec<T>, usize) {
        let total = items.len();
        let start = self.offset.min(total);
        let end = (start + self.limit).min(total);
        (items[start..end].to_vec(), total)
    }
}

/// 分页记账：把每个被截断集合的「截断前总数」与「是否被截断」汇总进 `page` 元信息。
#[derive(Debug, Default)]
struct PageReport {
    totals: serde_json::Map<String, serde_json::Value>,
    truncated: Vec<String>,
}

/// 被截断时的收窄提示（扁平列表与嵌套集合共用同一措辞）。
fn truncation_hint(total: usize, returned: usize) -> String {
    format!("已截断：共 {total} 条，本页 {returned} 条。用 offset/limit 翻页，或收窄查询")
}

impl PageReport {
    fn record(&mut self, path: &str, total: usize, returned: usize) {
        self.totals
            .insert(path.to_string(), serde_json::json!(total));
        if returned < total {
            self.truncated.push(path.to_string());
        }
    }

    /// 对 JSON 对象里的某个数组键分页（键不存在或不是数组时跳过）。
    fn page_key(
        &mut self,
        object: &mut serde_json::Map<String, serde_json::Value>,
        key: &str,
        path: &str,
        page: Page,
    ) {
        let (total, kept) = match object.get_mut(key) {
            Some(serde_json::Value::Array(items)) => {
                let total = items.len();
                let start = page.offset.min(total);
                let end = (start + page.limit).min(total);
                let kept = items[start..end].to_vec();
                *items = kept.clone();
                (total, kept)
            }
            _ => return,
        };
        if kept.len() < total {
            // 截断时给出明确的收窄提示，避免调用方以为「就这么多」。
            object.insert(
                format!("{key}_hint"),
                serde_json::json!(truncation_hint(total, kept.len())),
            );
        }
        self.record(path, total, kept.len());
    }

    fn value(&self, page: Page) -> serde_json::Value {
        serde_json::json!({
            "offset": page.offset,
            "limit": page.limit,
            "totals": self.totals,
            "truncated": self.truncated,
        })
    }
}

/// 构造 `get_planning_state` 的载荷：**逐层只给下一层的索引**，最内层才给细节。
///
/// 这是对「一条查询让上下文爆炸」的结构性修法：原来读整个文档会把每层工厂的每条
/// 机制一起倒出来（真实 py 上下文里一次自动规划就能写出 757 条机制），逐层索引 +
/// 每层分页后，任何一次调用的返回量都由 `page` 决定。抽成纯函数以便单测。
fn planning_state_value(snapshot: &DocSnapshot, page: Page) -> (serde_json::Value, PageReport) {
    let mut report = PageReport::default();
    let value = match snapshot {
        // 无 project：项目索引（复用 list_projects 的紧凑摘要）。
        DocSnapshot::Document(document) => {
            let projects = project_summary(document);
            let (kept, total) = page.slice(&projects);
            report.record("projects", total, kept.len());
            serde_json::json!({ "level": "document", "projects": kept })
        }
        // 单个 project：项目设置/规划偏好 + **工厂索引**（工厂细节要指名 factory）。
        DocSnapshot::Project(project) => {
            let factories = factory_summary(project);
            let (kept, total) = page.slice(&factories);
            report.record("factories", total, kept.len());
            serde_json::json!({
                "level": "project",
                "project": project.id.0,
                "name": project.name,
                "context_id": project.context_id,
                "settings": project.settings,
                "planning": project.planning,
                "factories": kept,
            })
        }
        // project + factory：工厂文档（重复集合按 page 截断）。
        DocSnapshot::Factory {
            project,
            factory,
            factory_document,
        } => {
            let mut document = match serde_json::to_value(factory_document) {
                Ok(serde_json::Value::Object(object)) => object,
                _ => serde_json::Map::new(),
            };
            for (key, path) in [
                ("mechanics", "mechanics"),
                ("targets", "targets"),
                ("target_expressions", "target_expressions"),
                ("external_inputs", "external_inputs"),
            ] {
                report.page_key(&mut document, key, path, page);
            }
            serde_json::json!({
                "level": "factory",
                "project": project,
                "factory": factory,
                "factory_document": serde_json::Value::Object(document),
            })
        }
    };
    (value, report)
}

/// Parameters for `get_planning_state` (all optional; omit for the whole document).
#[derive(Debug, serde::Deserialize, JsonSchema)]
struct PlanningStateParams {
    /// Project id (u64). Omit to return the project index.
    #[serde(default)]
    project: Option<u64>,
    /// Factory id (u64). Requires `project`; returns that factory's document.
    #[serde(default)]
    factory: Option<u64>,
    /// When set with `project` + `factory`, run a solve and include its result.
    #[serde(default)]
    recompute: bool,
    /// 每个集合的返回上限与偏移（默认 50、上限 1000）。
    #[serde(flatten)]
    page: PageParams,
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

    /// 分页参数：默认 50、下限 1、上限 1000；offset 原样传递。
    #[test]
    fn page_params_are_defaulted_and_clamped() {
        assert_eq!(PageParams::default().resolve().limit, 50);
        assert_eq!(PageParams::default().resolve().offset, 0);
        assert_eq!(
            PageParams {
                limit: Some(0),
                offset: None
            }
            .resolve()
            .limit,
            1
        );
        assert_eq!(
            PageParams {
                limit: Some(9_999),
                offset: None
            }
            .resolve()
            .limit,
            1000
        );
        let paged = PageParams {
            limit: Some(10),
            offset: Some(7),
        }
        .resolve();
        assert_eq!((paged.limit, paged.offset), (10, 7));
        // 切片语义：起始位置超出总数时返回空页，而不是 panic 或倒回开头。
        assert!(paged.slice(&[1, 2, 3]).0.is_empty());
    }

    /// `get_planning_state` 逐层只给下一层索引，最内层才给细节并分页——这是
    /// 「一条查询把上下文撑爆」的结构性修法（原来读整个文档会把每层工厂的每条
    /// 机制一起倒出来）。
    #[test]
    fn planning_state_is_level_by_level_and_paged() {
        let document: AppDocument = serde_json::from_value(serde_json::json!({
            "schema_version": metatorio_runtime::DOCUMENT_SCHEMA_VERSION,
            "projects": [{
                "id": 1,
                "name": "p",
                "factories": [{
                    "id": 2,
                    "name": "f",
                    "targets": [
                        { "id": 3, "flow": { "Item": { "id": "iron-plate", "quality": "normal" } }, "amount": 1.0 },
                        { "id": 4, "flow": { "Item": { "id": "copper-plate", "quality": "normal" } }, "amount": 2.0 },
                    ],
                    "mechanics": [
                        { "id": 5, "mechanic": { "type": "recipe" } },
                        { "id": 6, "mechanic": { "type": "recipe" } },
                        { "id": 7, "mechanic": { "type": "mining" } },
                    ]
                }]
            }]
        }))
        .unwrap();
        let project = document.projects[0].clone();
        let factory_document = project.factories[0].clone();
        let page = PageParams {
            limit: Some(2),
            offset: None,
        }
        .resolve();

        // 文档层：项目索引（没有工厂/机制明细）。
        let (value, report) = planning_state_value(&DocSnapshot::Document(document), page);
        assert_eq!(value["level"], "document");
        assert_eq!(value["projects"].as_array().unwrap().len(), 1);
        assert!(
            value["projects"][0]["mechanics"].is_number(),
            "项目索引里的 mechanics 是数量而不是明细数组：{}",
            value["projects"][0]
        );
        assert_eq!(report.totals["projects"], 1);
        assert!(report.truncated.is_empty(), "1 条项目不该被截断");

        // 项目层：设置/规划偏好 + 工厂索引（同样没有机制明细）。
        let (value, _) = planning_state_value(&DocSnapshot::Project(project), page);
        assert_eq!(value["level"], "project");
        assert_eq!(value["name"], "p");
        assert!(value.get("settings").is_some() && value.get("planning").is_some());
        assert_eq!(value["factories"].as_array().unwrap().len(), 1);
        assert!(
            value["factories"][0]["mechanics"].is_number(),
            "工厂索引里的 mechanics 是数量，项目层不该泄漏机制明细：{}",
            value["factories"][0]
        );
        assert!(value["factories"][0]["targets"].is_array());

        // 工厂层：给细节，但重复集合按 page 截断且如实上报（3 条机制 → 2 条）。
        let (value, report) = planning_state_value(
            &DocSnapshot::Factory {
                project: 1,
                factory: 2,
                factory_document,
            },
            page,
        );
        assert_eq!(value["level"], "factory");
        let document = &value["factory_document"];
        assert_eq!(document["mechanics"].as_array().unwrap().len(), 2);
        assert_eq!(document["targets"].as_array().unwrap().len(), 2);
        assert_eq!(report.totals["mechanics"], 3);
        assert_eq!(report.totals["targets"], 2);
        assert!(
            report.truncated.contains(&"mechanics".to_string()),
            "{:?}",
            report.truncated
        );
        assert!(
            !report.truncated.contains(&"targets".to_string()),
            "刚好放得下就不该标成截断"
        );
        assert!(
            document["mechanics_hint"]
                .as_str()
                .unwrap()
                .contains("共 3 条"),
            "被截断的集合要带收窄提示：{}",
            document["mechanics_hint"]
        );
        // page 元信息本身要能自证：截断前总数 + 被截断的集合名。
        let meta = report.value(page);
        assert_eq!(meta["limit"], 2);
        assert_eq!(meta["totals"]["mechanics"], 3);
        assert_eq!(meta["truncated"][0], "mechanics");
    }
}
