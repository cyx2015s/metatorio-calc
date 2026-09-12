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

use std::net::{IpAddr, SocketAddr};

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
use metatorio_core::{BeaconConfig, DualVar, IdWithQuality, ModuleConfig};
use metatorio_runtime::document::AutoBeaconPlan;
use metatorio_runtime::message::{
    AppMessage, ApplicationAction, DeleteDecision, FactoryAction, FactoryContextAction,
    FactoryTemplate, FlowAction, PlanningAction, ProjectAction, RuntimeCommand, SolveAction,
};
use metatorio_runtime::{FactoryId, MechanicId, ProjectId};

/// Default loopback port for the MCP endpoint (`--mcp-port` /
/// `METATORIO_MCP_PORT` 可覆盖）。
pub const DEFAULT_MCP_PORT: u16 = 8765;

/// 默认只监听回环：这个端点能建项目、改目标、跑规划，默认不该被局域网里任何设备碰到。
/// 要手机/其它设备接入就显式 `--mcp-bind <本机 IP>`（或 `0.0.0.0`），并且**必须**配
/// token（启动前校验，见 bin 的 `validate`）。
pub const DEFAULT_MCP_BIND: &str = "127.0.0.1";

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
        and include its result (its mechanics/flows are paged the same way).  \
        With `project`+`factory` the response also carries `auto_plan` when that \
        factory has an asynchronous auto-plan (started by the `auto_plan` tool): \
        `{status: running|done|failed, revision?, result?, error?}` — poll this to \
        collect the result.  \
        Add `mechanic` (an id from that factory's `mechanics`, so it also needs \
        project + factory) to go one level deeper and get **one mechanic's flow \
        conversion**: its full `config` (type/machine/recipe/modules/beacons/fuel) plus \
        `inputs`/`outputs` — the per-second amounts that mechanic consumes and produces \
        at coefficient 1, already including machine speed, module and beacon effects \
        (the same expansion the solver uses).  Use it instead of inferring what a \
        mechanic does from its name.  \
        All flow amounts are per second (time-scale only affects display)."
    )]
    async fn get_planning_state(
        &self,
        Parameters(params): Parameters<PlanningStateParams>,
    ) -> Result<CallToolResult, McpError> {
        let app = self.app.clone();
        let page = params.page.resolve();
        let project = params.project.map(ProjectId);
        let factory = params.factory.map(FactoryId);
        // `recompute` / `mechanic` 只在 project + factory 同时给出时才有意义：其余
        // 组合显式报错，而不是静默忽略（agent 之前无法察觉）。
        if params.recompute && (project.is_none() || factory.is_none()) {
            return Err(McpError::invalid_params(
                "recompute 需要同时提供 project 与 factory".to_string(),
                None,
            ));
        }
        if params.mechanic.is_some() && (project.is_none() || factory.is_none()) {
            return Err(McpError::invalid_params(
                "mechanic 需要同时提供 project 与 factory（机制属于某个工厂）".to_string(),
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

        // 1.5) 指定 mechanic：再下一层，**聚焦到这一个机制**（配置 + 系数 = 1 的每秒
        //      产/耗）。机制在文档里只有 machine/recipe/modules 这些零件，agent 光看
        //      机制名推不出它到底消耗/产出什么，尤其是插件、插件塔、机器速度都参与之后。
        if let Some(mechanic) = params.mechanic {
            let (mechanic_project, mechanic_factory, factory_document) = match &snapshot {
                DocSnapshot::Factory {
                    project,
                    factory,
                    factory_document,
                } => (*project, *factory, factory_document),
                // 上面已要求 project + factory 同时给出；走到这里说明内部状态不一致。
                _ => {
                    return Err(McpError::internal_error(
                        "mechanic 需要 project + factory".to_string(),
                        None,
                    ))
                }
            };
            let mut report = PageReport::default();
            let mut object = match mechanic_level_value(
                &app,
                mechanic_project,
                mechanic_factory,
                factory_document,
                mechanic,
                page,
                &mut report,
            )
            .await?
            {
                serde_json::Value::Object(object) => object,
                _ => serde_json::Map::new(),
            };
            // `recompute` 与 `mechanic` 同时给出时，求解结果也算出来了，别丢掉。
            if let Some(mut solve) = solve {
                if let Some(status) = solve.get_mut("status").and_then(|s| s.as_object_mut()) {
                    if let Some(solved) = status.get_mut("solved").and_then(|s| s.as_object_mut()) {
                        report.page_key(solved, "mechanics", "solve.mechanics", page);
                        report.page_key(solved, "flows", "solve.flows", page);
                    }
                }
                object.insert("solve".to_string(), solve);
            }
            object.insert(
                "revision".to_string(),
                serde_json::Value::Number(revision.into()),
            );
            object.insert("page".to_string(), report.value(page));
            return Ok(CallToolResult::structured(serde_json::Value::Object(
                object,
            )));
        }

        let (mut value, mut report) = planning_state_value(&snapshot, page);
        // 工厂层：附上该工厂的**异步自动规划**状态（如果有），供 `auto_plan` 的
        // 调用方轮询。这是「稍后查这个 project + factory」的落点：状态与结果都由
        // 那次规划自己写入，不用猜、也不会与当前文档的 recompute 混淆。
        if let DocSnapshot::Factory {
            project, factory, ..
        } = &snapshot
        {
            let status = app
                .state::<AppState>()
                .auto_plans
                .lock()
                .ok()
                .and_then(|plans| {
                    plans
                        .get(&(ProjectId(*project), FactoryId(*factory)))
                        .cloned()
                });
            if let Some(status) = status {
                let auto_plan = match status {
                    crate::AutoPlanState::Running => serde_json::json!({ "status": "running" }),
                    crate::AutoPlanState::Failed(error) => {
                        serde_json::json!({ "status": "failed", "error": error })
                    }
                    crate::AutoPlanState::Done { revision, result } => {
                        let mut result =
                            serde_json::to_value(&*result).unwrap_or(serde_json::Value::Null);
                        if let Some(solved) = result
                            .get_mut("status")
                            .and_then(|status| status.as_object_mut())
                            .and_then(|status| status.get_mut("solved"))
                            .and_then(|solved| solved.as_object_mut())
                        {
                            report.page_key(solved, "mechanics", "auto_plan.mechanics", page);
                            report.page_key(solved, "flows", "auto_plan.flows", page);
                        }
                        serde_json::json!({
                            "status": "done",
                            "revision": revision,
                            "result": result,
                        })
                    }
                };
                if let Some(object) = value.as_object_mut() {
                    object.insert("auto_plan".to_string(), auto_plan);
                }
            }
        }
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

    /// 一站式入口：建项目/工厂 + 配好目标/星球/品质/插件/插件塔/外部输入，然后
    /// **立刻返回**并异步跑自动规划。
    ///
    /// 群里的实测反馈：每次手拼 `dispatch` 序列既费人又费 AI（而且容易漏配严格
    /// 供给）。这个入口把那条序列固定下来，并把「跑得久」的规划甩到后台。
    #[tool(
        description = "ONE-SHOT planning entry point: create a project + factory, configure \
        targets / planet / major quality / modules / beacons / external inputs, then kick \
        off auto-planning **asynchronously and return immediately**.  \
        Required: `targets` = [{ item, quality?, amount }] — `item` accepts a raw id \
        (`iron-plate`) or a localized name (`铁板`); separators are ignored; a typo or a \
        non-item name is rejected with candidates (use `localized_names` if unsure).  \
        **Every hand-written name is validated against the active context before \
        anything is created** (`item`, `modules.exclude`, `beacons[].beacon`, \
        `beacons[].modules[].module`, `external_inputs[].flow`): one wrong name fails \
        the whole call with candidates and creates nothing — a name that silently does \
        nothing (or writes garbage into the document) is worse than an error.  \
        Optional: `planet`, `major_quality`, `modules` = {best, quality, exclude[]}, \
        `beacons` = [{beacon:{id,quality}, count?, share?, modules:[{module,count?}]}], \
        `external_inputs` = [{flow, penalty?}] (this is how you supply raw materials — \
        auto-planning is ALWAYS strict-source and there is no toggle), `project_name`, \
        `factory_name`, `context_id`, `request_id` (idempotent retry).  \
        The response returns the created `project`/`factory` ids plus \
        `{\"auto_plan\": {\"status\": \"running\"}}` and a `poll` hint; call \
        `get_planning_state` with that project + factory to read \
        `auto_plan.status` (`running`/`done`/`failed`), and the solve result once done.  \
        All flow amounts are per second.  Prefer this over hand-writing the dispatch \
        sequence unless you need something it does not expose."
    )]
    async fn auto_plan(
        &self,
        Parameters(params): Parameters<AutoPlanParams>,
    ) -> Result<CallToolResult, McpError> {
        let app = self.app.clone();
        if params.targets.is_empty() {
            return Err(McpError::invalid_params(
                "targets 至少给一个目标".to_string(),
                None,
            ));
        }
        // 幂等回放：同一个 request_id 直接返回上次载荷（含 ids），不重复建项目。
        if let Some(request_id) = &params.request_id {
            let cached = app
                .state::<AppState>()
                .dispatch_cache
                .lock()
                .ok()
                .and_then(|cache| cache.get(request_id));
            if let Some((mut payload, false)) = cached {
                if let serde_json::Value::Object(object) = &mut payload {
                    object.insert("idempotent_replay".into(), serde_json::Value::Bool(true));
                }
                return Ok(CallToolResult::structured(payload));
            }
        }
        // 1) 目标名 → id：用当前/指定的上下文目录索引解析（只接受精确的物品原型）。
        let state = app.state::<AppState>();
        let context_id = crate::resolve_context_id(&state, params.context_id.as_deref())
            .map_err(|error| McpError::invalid_params(error, None))?;
        let index = crate::catalog_index_for(&state, &context_id)
            .await
            .map_err(|error| McpError::invalid_params(format!("读取目录失败: {error}"), None))?;
        let targets = resolve_auto_plan_targets(&index, &params.targets)
            .map_err(|error| McpError::invalid_params(error, None))?;
        // 手写名字（插件/插件塔/外部输入）全部先过一遍：不通过就一个对象都不建。
        validate_auto_plan_names(&index, &params)
            .map_err(|error| McpError::invalid_params(error, None))?;

        // 2) 建项目（拿 id）→ 配置 → 触发。**配置阶段任何一步失败都回滚刚建的项目**：
        //    这个项目是本次调用自己建的、调用方还不知道它存在，留下一个「只配了一半」
        //    的项目比直接报错更糟（实测：插件名写错一个字母就会留下半成品）。
        let outcome = apply_consistency_only(
            &app,
            AppMessage::Application(ApplicationAction::NewProject {
                name: params
                    .project_name
                    .clone()
                    .unwrap_or_else(|| "自动规划".to_string()),
            }),
        )
        .await?;
        let project = match outcome.created.projects.first() {
            Some(project) => *project,
            None => {
                return Err(McpError::internal_error(
                    "新建项目没有返回 id".to_string(),
                    None,
                ))
            }
        };

        let configured = async {
            if let Some(context) = &params.context_id {
                apply_consistency_only(
                    &app,
                    AppMessage::Project {
                        project,
                        action: ProjectAction::SetContext {
                            context: Some(context.clone()),
                        },
                    },
                )
                .await?;
            }
            let factory = match apply_consistency_only(
                &app,
                AppMessage::Project {
                    project,
                    action: ProjectAction::AddFactory {
                        name: params
                            .factory_name
                            .clone()
                            .unwrap_or_else(|| "主工厂".to_string()),
                        template: FactoryTemplate::DefaultMechanics,
                    },
                },
            )
            .await?
            .created
            .factories
            .first()
            .copied()
            {
                Some(factory) => factory,
                None => {
                    return Err(McpError::internal_error(
                        "新建工厂没有返回 id".to_string(),
                        None,
                    ))
                }
            };
            // 序列里最后一条是 `solve: auto-plan`：**不在请求里同步跑**，否则一次
            // 调用要等几分钟（py 那种规模实测 70s+）。
            let mut messages = auto_plan_body_messages(&params, project, factory, &targets);
            let trigger = messages.pop();
            let mut created_targets = Vec::new();
            let mut created_inputs = Vec::new();
            for message in &messages {
                let outcome = apply_consistency_only(&app, message.clone()).await?;
                created_targets.extend(outcome.created.targets.iter().map(|id| id.0));
                created_inputs.extend(outcome.created.external_inputs.iter().map(|id| id.0));
            }

            // 3) 触发异步自动规划（状态写进 AppState，供 agent 稍后查询）。
            let mut status = "running";
            let mut trigger_commands = Vec::new();
            if let Some(message) = trigger {
                // 只走 reducer：AutoPlan 命令由后台任务执行（它不是一致性命令）。
                trigger_commands = apply_consistency_only(&app, message).await?.commands;
            }
            let trigger_command = trigger_commands
                .into_iter()
                .find(|command| matches!(command, RuntimeCommand::AutoPlan { .. }));
            match trigger_command {
                Some(command) => crate::spawn_auto_plan(&app, project, factory, command),
                None => {
                    // reducer 没发出 AutoPlan（例如项目/工厂刚被别处删掉）：如实报告。
                    status = "failed";
                    if let Ok(mut plans) = app.state::<AppState>().auto_plans.lock() {
                        plans.insert(
                            (project, factory),
                            crate::AutoPlanState::Failed("未能触发自动规划命令".to_string()),
                        );
                    }
                }
            }
            Ok::<_, McpError>((factory, status, created_targets, created_inputs))
        }
        .await;
        let (factory, status, created_targets, created_inputs) = match configured {
            Ok(configured) => configured,
            Err(error) => {
                // 回滚：只删本次调用刚建的项目。回滚自己也失败时**两个事实都报**，
                // 不能让调用方以为项目已经收干净了。
                let rolled_back = match apply_consistency_only(
                    &app,
                    AppMessage::Application(ApplicationAction::DeleteProject {
                        project,
                        decision: DeleteDecision::Confirm,
                    }),
                )
                .await
                {
                    Ok(_) => true,
                    Err(rollback) => {
                        eprintln!("auto_plan rollback failed for project {project:?}: {rollback}");
                        false
                    }
                };
                let suffix = if rolled_back {
                    format!("（已回滚：本次新建的项目 {} 已删除）", project.0)
                } else {
                    format!("（回滚失败：项目 {} 仍留在文档里，请手动删除）", project.0)
                };
                let message = if error.message.is_empty() {
                    format!("auto_plan 配置失败{suffix}")
                } else {
                    format!("{}{suffix}", error.message)
                };
                return Err(McpError::invalid_params(message, None));
            }
        };
        let modules_best = params.modules.as_ref().is_some_and(|modules| modules.best);
        let modules_excluded = params
            .modules
            .as_ref()
            .map(|modules| modules.exclude.len())
            .unwrap_or(0);
        let payload = serde_json::json!({
            "project": project.0,
            "factory": factory.0,
            "created": {
                "project": project.0,
                "factory": factory.0,
                "targets": created_targets,
                "external_inputs": created_inputs,
            },
            "applied": {
                "planet": params.planet,
                "major_quality": params.major_quality,
                "context_id": params.context_id,
                "targets": targets.iter().map(|(item, amount)| serde_json::json!({
                    "item": item.id,
                    "quality": item.quality,
                    "amount": amount,
                })).collect::<Vec<_>>(),
                "external_inputs": params.external_inputs.len(),
                "modules_best": modules_best,
                "modules_excluded": modules_excluded,
                "beacons": params.beacons.len(),
            },
            "auto_plan": { "status": status },
            "poll": {
                "tool": "get_planning_state",
                "arguments": { "project": project.0, "factory": factory.0 },
                "hint": "规划在后台跑：稍后（普通上下文 2~5s 起，py 这类大上下文要几分钟）\
                用 get_planning_state 查这个 project + factory，读 auto_plan.status\
                （running/done/failed）；done 时求解结果在 auto_plan.result 里（已分页）。",
            },
        });
        if let Some(request_id) = params.request_id {
            if let Ok(mut cache) = app.state::<AppState>().dispatch_cache.lock() {
                cache.insert(request_id, payload.clone(), false);
            }
        }
        Ok(CallToolResult::structured(payload))
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
        typo-tolerant candidates (queries of 3+ characters, or 2 characters when the \
        query is non-ASCII — a two-character Chinese name is a whole word), each with an edit \
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

/// 一个目标：物品（id **或**本地化名）+ 可选品质 + 每秒速率。
#[derive(Debug, Clone, serde::Deserialize, JsonSchema)]
struct AutoPlanTarget {
    /// 物品名：原型 id（`iron-plate`）或本地化名（`铁板`）都行；分隔符不敏感。
    item: String,
    /// 品质（`normal` / `uncommon` / …）；省略 = `normal`。
    #[serde(default)]
    quality: Option<String>,
    /// 目标速率（每秒；项目 time-scale 只影响显示）。
    amount: f64,
}

/// 插件策略。
#[derive(Debug, Default, Clone, serde::Deserialize, JsonSchema)]
struct AutoPlanModules {
    /// 用「每类别 tier 最高的插件」填充枚举列表（需要 `quality`）。
    #[serde(default)]
    best: bool,
    /// `best` 使用的品质；省略 = 工厂主品质。
    #[serde(default)]
    quality: Option<String>,
    /// 从枚举列表里剔除的插件（在 `best` 之后应用，例如排除效率 3）。
    #[serde(default)]
    exclude: Vec<IdWithQuality>,
}

/// 插件塔方案（自动规划叠加的插件塔配置）。
#[derive(Debug, Clone, serde::Deserialize, JsonSchema)]
struct AutoPlanBeacon {
    /// 插件塔本体（id + 品质）。
    beacon: IdWithQuality,
    /// 插件塔数量；省略 = 1。
    #[serde(default)]
    count: Option<usize>,
    /// 共享比例（平均一个塔覆盖几台机器）；省略 = 1.0。
    #[serde(default)]
    share: Option<f64>,
    /// 塔内插件（数量是「塔内插件数」，不是塔数量）。
    #[serde(default)]
    modules: Vec<AutoPlanBeaconModule>,
}

#[derive(Debug, Clone, serde::Deserialize, JsonSchema)]
struct AutoPlanBeaconModule {
    module: IdWithQuality,
    #[serde(default)]
    count: Option<usize>,
}

/// 外部输入：手动指定原料来源（**这才是「放宽严格供给」的正确做法**——
/// 自动规划总是严格供给，缺什么就在这里声明什么，而不是关掉严格供给）。
#[derive(Debug, Clone, serde::Deserialize, JsonSchema)]
struct AutoPlanExternalInput {
    /// 流（`{"Item":{"id":"iron-plate","quality":"normal"}}` / `{"Fluid":{…}}` / `"Electricity"`）。
    flow: DualVar,
    /// 惩罚系数；省略 = 1.0。
    #[serde(default)]
    penalty: Option<f64>,
}

/// `auto_plan` 的参数：把群里 bot 实际用过的配置序列收敛成**一个入口**。
///
/// 一次调用内部依次执行：新建项目 → （可选绑定上下文）→ 新建工厂 →
/// 星球/主品质 → 目标（多个）→ 外部输入 → 插件策略 → 插件塔方案 → 触发自动规划。
/// 与逐条 `dispatch` 相比，人只需记一个工具、AI 只需一次调用。
#[derive(Debug, Clone, serde::Deserialize, JsonSchema)]
struct AutoPlanParams {
    /// 目标列表（至少一个）。
    targets: Vec<AutoPlanTarget>,
    /// 项目名；省略 = “自动规划”。
    #[serde(default)]
    project_name: Option<String>,
    /// 工厂名；省略 = “主工厂”。
    #[serde(default)]
    factory_name: Option<String>,
    /// 星球（工厂设置，如 `nauvis` / `vulcanus`）；省略 = 保持新工厂默认（nauvis）。
    #[serde(default)]
    planet: Option<String>,
    /// 主品质（工厂设置）；省略 = `normal`。
    #[serde(default)]
    major_quality: Option<String>,
    /// 插件策略（最佳 / 剔除）。
    #[serde(default)]
    modules: Option<AutoPlanModules>,
    /// 枚举插件塔方案。
    #[serde(default)]
    beacons: Vec<AutoPlanBeacon>,
    /// 外部输入（手动指定原料来源）。
    #[serde(default)]
    external_inputs: Vec<AutoPlanExternalInput>,
    /// 绑定到某个游戏上下文 id；省略 = 当前激活上下文。
    #[serde(default)]
    context_id: Option<String>,
    /// 幂等键：同一个 id 的重复调用只应用一次，重试直接拿回上次载荷。
    #[serde(default)]
    request_id: Option<String>,
}

/// 组装 `auto_plan` 在「项目与工厂都已建好」之后要依次执行的消息序列
/// （纯函数：`project`/`factory` id 由工具按 `created` 回填后传入）。
///
/// **这里不建工厂**：工厂必须由工具先建，才能拿到 id 去组装工厂级消息；本函数若
/// 再发一条 `add-factory`，一个 `auto_plan` 调用就会给项目留下两个工厂（一个是
/// 刚配好的、一个是空模板）——实测踩到过：项目里凭空多出一个 12 机制的 `主工厂`。
///
/// 顺序有讲究：
/// 1. **星球/主品质在目标之前**：它们决定求解环境（太阳能系数、允许的品质档）；
/// 2. 目标 → 外部输入：先确定「要什么」，再声明「从哪来」；
/// 3. `best` 模块（整体替换枚举列表）**在剔除之前**，否则剔除会被覆盖；
/// 4. 插件塔方案次之，最后才是 `solve: auto-plan` 触发。
fn auto_plan_body_messages(
    params: &AutoPlanParams,
    project: ProjectId,
    factory: FactoryId,
    targets: &[(IdWithQuality, f64)],
) -> Vec<AppMessage> {
    let mut messages = Vec::new();
    if let Some(planet) = &params.planet {
        messages.push(AppMessage::Factory {
            project,
            factory,
            action: FactoryAction::Context(FactoryContextAction::SetPlanet {
                planet: Some(planet.clone()),
            }),
        });
    }
    if let Some(quality) = &params.major_quality {
        messages.push(AppMessage::Factory {
            project,
            factory,
            action: FactoryAction::Context(FactoryContextAction::SetMajorQuality {
                quality: quality.clone(),
            }),
        });
    }
    for (item, amount) in targets {
        messages.push(AppMessage::Factory {
            project,
            factory,
            action: FactoryAction::Flow(FlowAction::AddToTarget {
                flow: DualVar::Item(item.clone()),
                amount: *amount,
            }),
        });
    }
    for input in &params.external_inputs {
        messages.push(AppMessage::Factory {
            project,
            factory,
            action: FactoryAction::Flow(FlowAction::AddToExternalInput {
                flow: input.flow.clone(),
                penalty: input.penalty.unwrap_or(1.0),
            }),
        });
    }
    if let Some(modules) = &params.modules {
        if modules.best {
            let quality = modules
                .quality
                .clone()
                .or_else(|| params.major_quality.clone())
                .unwrap_or_else(|| "normal".to_string());
            messages.push(AppMessage::Project {
                project,
                action: ProjectAction::Planning(PlanningAction::UseBestModules { quality }),
            });
        }
        for module in &modules.exclude {
            messages.push(AppMessage::Project {
                project,
                action: ProjectAction::Planning(PlanningAction::RemoveEnumeratedModule {
                    module: module.clone(),
                }),
            });
        }
    }
    for (index, beacon) in params.beacons.iter().enumerate() {
        // 新项目的枚举插件塔列表从空开始，因此这里逐个 append，index 即序号。
        messages.push(AppMessage::Project {
            project,
            action: ProjectAction::Planning(PlanningAction::AddEnumeratedBeacon),
        });
        let plan = AutoBeaconPlan {
            module_config: ModuleConfig {
                modules: Vec::new(),
                beacons: vec![BeaconConfig {
                    beacon: beacon.beacon.clone(),
                    count: beacon.count.unwrap_or(1),
                    share: beacon.share.unwrap_or(1.0),
                    modules: beacon
                        .modules
                        .iter()
                        .map(|entry| (entry.module.clone(), entry.count.unwrap_or(1)))
                        .collect(),
                }],
            },
        };
        messages.push(AppMessage::Project {
            project,
            action: ProjectAction::Planning(PlanningAction::SetEnumeratedBeacon {
                beacon: index,
                plan,
            }),
        });
    }
    messages.push(AppMessage::Factory {
        project,
        factory,
        action: FactoryAction::Solve(SolveAction::AutoPlan),
    });
    messages
}

/// 把目标里的物品名解析成 `(IdWithQuality, amount)`：接受原型 id 或本地化名
/// （分隔符不敏感），但**只接受精确命中的物品原型**——不猜：名字打错或指向配方时
/// 报错并附候选（含错拼候选），因为错误的名字能通过校验、却会让计划悄悄跑偏。
fn resolve_auto_plan_targets(
    index: &crate::CatalogIndex,
    targets: &[AutoPlanTarget],
) -> Result<Vec<(IdWithQuality, f64)>, String> {
    let mut resolved = Vec::new();
    for target in targets {
        if !(target.amount.is_finite() && target.amount > 0.0) {
            return Err(format!(
                "目标物品「{}」的 amount 必须是正数（每秒速率）",
                target.item
            ));
        }
        let name = require_index_entry(index, &["item"], "目标物品", &target.item)?;
        let quality = target
            .quality
            .clone()
            .unwrap_or_else(|| "normal".to_string());
        resolved.push((IdWithQuality::new(name, quality), target.amount));
    }
    Ok(resolved)
}

/// 「近似候选」提示：把精确/模糊/错拼命中压成短标签，附在报错里。
fn name_candidates(outcome: &crate::ResolvedQuery) -> Vec<String> {
    let mut hints: Vec<String> = outcome
        .exact
        .iter()
        .chain(outcome.partial.iter())
        .take(5)
        .map(|hit| format!("{} [{}]", hit.name, hit.kind))
        .collect();
    hints.extend(outcome.typo.iter().take(3).map(|hit| {
        format!(
            "{} [{}]（错拼? d={}）",
            hit.name,
            hit.kind,
            hit.distance.unwrap_or(0)
        )
    }));
    hints
}

/// 手写名字必须**精确命中** `kinds` 里的某一种原型，命中则返回规范原型名，否则报错
/// 并附候选（id / 本地化名 / 错拼都行，分隔符不敏感）。
///
/// 为什么要在入口挡：`auto_plan` 的参数是**人和 AI 手打的名字**，而写错的名字不一定
/// 会报错——实测两种情况都很危险：
/// - `remove-enumerated-module` 对不存在的插件是**静默 no-op**（`retain` 找不到就报
///   「无变化」），调用方以为「已排除」；
/// - `add-to-external-input` 更直接把不存在的物品**原样写进文档**，自动规划照常
///   「成功」，那条外部输入其实什么也没接上。
fn require_index_entry(
    index: &crate::CatalogIndex,
    kinds: &[&str],
    label: &str,
    name: &str,
) -> Result<String, String> {
    let outcome = crate::resolve_index_entry(&index.entries, name, 8);
    if let Some(hit) = outcome
        .exact
        .iter()
        .find(|hit| kinds.contains(&hit.kind.as_str()))
    {
        return Ok(hit.name.clone());
    }
    let hints = name_candidates(&outcome);
    Err(if hints.is_empty() {
        format!("{label}「{name}」不存在于当前游戏上下文：用 localized_names 查一下正确名字")
    } else {
        format!(
            "{label}「{name}」不存在于当前游戏上下文，近似候选：{}",
            hints.join("、")
        )
    })
}

/// `auto_plan` 里**所有手写名字的参数**在「建任何东西之前」统一过一遍：插件剔除项、
/// 插件塔与其插件、外部输入的物品/流体/实体。
///
/// 校验不过就整个调用失败、一个对象都不建——这正是组合入口该有的原子性；等到配置
/// 中途才报错，就得靠回滚来收拾半成品（回滚仍然保留，用来兜住 reducer 侧的失败）。
/// 虚拟流（电/热/污染/燃料流）不是原型，不校验。
fn validate_auto_plan_names(
    index: &crate::CatalogIndex,
    params: &AutoPlanParams,
) -> Result<(), String> {
    if let Some(modules) = &params.modules {
        for module in &modules.exclude {
            require_index_entry(index, &["module", "item"], "要剔除的插件", &module.id)?;
        }
    }
    for (position, beacon) in params.beacons.iter().enumerate() {
        let label = format!("第 {} 个插件塔", position + 1);
        require_index_entry(index, &["beacon", "entity"], &label, &beacon.beacon.id)?;
        for module in &beacon.modules {
            require_index_entry(
                index,
                &["module", "item"],
                &format!("{label}里的插件"),
                &module.module.id,
            )?;
        }
    }
    for input in &params.external_inputs {
        match &input.flow {
            DualVar::Item(item) => {
                require_index_entry(index, &["item"], "外部输入物品", &item.id)?;
            }
            DualVar::Fluid { name, .. } => {
                require_index_entry(index, &["fluid"], "外部输入流体", name)?;
            }
            DualVar::Entity(entity) => {
                require_index_entry(index, &["entity"], "外部输入实体", &entity.id)?;
            }
            // 电 / 热 / 污染 / 燃料流 / 自定义流都是虚拟流，没有对应原型。
            _ => {}
        }
    }
    Ok(())
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

/// 走一步 reducer（短锁），并且**只执行便宜的收敛命令**（品质上限 / 机器兼容 /
/// 插件钳制），跳过 `Recompute`/`Persist`/`AutoPlan`——组合入口先要把文档配好，
/// 而每配一步就跑一次整厂求解会让「立即返回」变成几分钟（py 实测一次 70s+）。
/// 最后那次 `solve: auto-plan` 会统一落盘与重解。
async fn apply_consistency_only<R: Runtime + 'static>(
    app: &AppHandle<R>,
    message: AppMessage,
) -> Result<metatorio_runtime::state::DispatchResult, McpError> {
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
    .map_err(|error| McpError::invalid_params(format!("auto_plan 配置失败: {error}"), None))?;
    let state = app.state::<AppState>();
    for command in &outcome.commands {
        if crate::is_consistency_command(command) {
            let _ = execute_command(app, &state, command).await;
        }
    }
    // 文档变了就通知 GUI：外部 agent 建的项目要立刻出现在界面上。
    let _ = app.emit("document-changed", outcome.revision);
    Ok(outcome)
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
    /// Mechanic id (from that factory's `mechanics`). Requires `project` + `factory`;
    /// returns just this mechanic's config plus its per-second inputs/outputs at
    /// coefficient 1.
    #[serde(default)]
    mechanic: Option<u64>,
    /// When set with `project` + `factory`, run a solve and include its result.
    #[serde(default)]
    recompute: bool,
    /// 每个集合的返回上限与偏移（默认 50、上限 1000）。
    #[serde(flatten)]
    page: PageParams,
}

/// 「机制层」载荷：一个机制的**配置** + 它在**系数 = 1** 时的每秒产/耗。
///
/// 为什么要这一层：文档里的机制只有 `machine` / `recipe` / `module_config` 这些零件，
/// agent 光看机制名（甚至看零件）也推不出「一个系数到底消耗什么、产出什么」——机器
/// 速度、插件、插件塔都参与之后更是如此。这里给的是 GUI 机制卡显示的**同一份展开
/// 结果**（[`crate::mechanic_flow_for`]），不是另写一套近似。
async fn mechanic_level_value(
    app: &AppHandle,
    project: u64,
    factory: u64,
    factory_document: &metatorio_runtime::FactoryDocument,
    mechanic: u64,
    page: Page,
    report: &mut PageReport,
) -> Result<serde_json::Value, McpError> {
    let entry = factory_document
        .mechanics
        .iter()
        .find(|entry| entry.id == MechanicId(mechanic))
        .ok_or_else(|| {
            McpError::invalid_params(
                format!(
                    "机制 {mechanic} 不在工厂 {factory} 里：先 get_planning_state \
                     {{project, factory}} 读 mechanics 列表拿 id"
                ),
                None,
            )
        })?;
    let config = serde_json::to_value(&entry.mechanic).unwrap_or(serde_json::Value::Null);
    let flow = crate::mechanic_flow_for(
        &app.state::<AppState>(),
        ProjectId(project),
        FactoryId(factory),
        MechanicId(mechanic),
    )
    .await
    .map_err(|error| {
        McpError::invalid_params(format!("读取机制 {mechanic} 的流失败: {error}"), None)
    })?;

    let (inputs, outputs) = split_mechanic_flow(&flow);
    let (inputs, inputs_total) = page.slice(&inputs);
    let (outputs, outputs_total) = page.slice(&outputs);
    report.record("mechanic.inputs", inputs_total, inputs.len());
    report.record("mechanic.outputs", outputs_total, outputs.len());

    let encode = |items: Vec<(DualVar, f64)>| {
        items
            .into_iter()
            .map(|(flow, amount)| serde_json::json!({ "flow": flow, "amount": amount }))
            .collect::<Vec<_>>()
    };
    let mut object = serde_json::Map::new();
    object.insert("level".to_string(), serde_json::json!("mechanic"));
    object.insert("project".to_string(), serde_json::json!(project));
    object.insert("factory".to_string(), serde_json::json!(factory));
    object.insert("mechanic".to_string(), serde_json::json!(mechanic));
    object.insert("enabled".to_string(), serde_json::json!(entry.enabled));
    object.insert("config".to_string(), config);
    object.insert(
        "rate_note".to_string(),
        serde_json::json!(
            "系数（求解里的 amount）= 1 时每秒的量：inputs 是消耗、outputs 是产出，\
             已含机器速度、插件与插件塔效果（与求解同一份展开）"
        ),
    );
    object.insert(
        "inputs".to_string(),
        serde_json::json!(encode(inputs.clone())),
    );
    object.insert(
        "outputs".to_string(),
        serde_json::json!(encode(outputs.clone())),
    );
    // 与其它集合同一口径：截断了就说清怎么收窄，绝不静默丢。
    if inputs.len() < inputs_total {
        object.insert(
            "inputs_hint".to_string(),
            serde_json::json!(truncation_hint(inputs_total, inputs.len())),
        );
    }
    if outputs.len() < outputs_total {
        object.insert(
            "outputs_hint".to_string(),
            serde_json::json!(truncation_hint(outputs_total, outputs.len())),
        );
    }
    Ok(serde_json::Value::Object(object))
}

/// 把展开出来的带符号流拆成 `(消耗, 产出)`，两边都是**正数**：符号改由数组名承载。
///
/// 展开结果里正数是产出、负数是消耗（见 `mechanic_flow` 的约定），直接给 LLM 看
/// `-2.0` 容易被读成「产出 2」。接近 0 的浮点残渣（`|v| <= 1e-12`）在展开侧已经
/// 过滤过，这里不再重复判断。
fn split_mechanic_flow(flow: &[(DualVar, f64)]) -> (Vec<(DualVar, f64)>, Vec<(DualVar, f64)>) {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for (flow, amount) in flow {
        if *amount < 0.0 {
            inputs.push((flow.clone(), -amount));
        } else {
            outputs.push((flow.clone(), *amount));
        }
    }
    (inputs, outputs)
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

/// MCP 端点的启动配置（由 `Options` 组装后传入，见 `crate::run`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    /// 监听地址：`127.0.0.1`（默认）/ 本机 IP / `0.0.0.0`，或 `localhost`。
    pub bind: String,
    /// 监听端口。
    pub port: u16,
    /// Bearer token；`None`/空 = 不鉴权（只允许出现在回环绑定上）。
    pub token: Option<String>,
    /// 额外允许的 `Host`（主机名/mDNS 名）；IP 由 `bind` 自动允许。
    pub allow_hosts: Vec<String>,
}

/// 把 `--mcp-bind` 解析成「监听地址 + Host 白名单」。
///
/// 白名单来自 rmcp 的 DNS-rebinding 防护（默认只认回环 Host）：绑到具体 IP 时把该 IP
/// 加进去，客户端用 `http://<那个 IP>:<port>/mcp` 就不会被 Host 校验拦下。绑到
/// `0.0.0.0`/`::` 时无从枚举本机地址，返回 `None` 表示**关闭**这项校验——此时调用方
/// 已明确要求对所有网卡开放，访问控制只剩 token（启动日志会写明这一点）。
///
/// `pub` 是为了让 bin 的 `validate` 复用同一份解析（非回环必须有 token 的判断要用它），
/// 而不是各写一份「什么算回环」。
pub fn resolve_bind(
    bind: &str,
    allow_hosts: &[String],
) -> Result<(IpAddr, Option<Vec<String>>), String> {
    let bind = bind.trim();
    if bind.is_empty() {
        return Err("--mcp-bind 不能为空".to_string());
    }
    let addr: IpAddr = if bind.eq_ignore_ascii_case("localhost") {
        IpAddr::from([127, 0, 0, 1])
    } else {
        bind.parse().map_err(|_| {
            format!(
                "--mcp-bind `{bind}` 不是合法地址：只接受 IP 字面量（`127.0.0.1` / \
                 `0.0.0.0` / `192.168.1.23`）或 `localhost`"
            )
        })?
    };
    if addr.is_unspecified() {
        return Ok((addr, None));
    }
    let mut hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    hosts.push(addr.to_string());
    for host in allow_hosts {
        let host = host.trim();
        if !host.is_empty() && !hosts.iter().any(|existing| existing == host) {
            hosts.push(host.to_string());
        }
    }
    Ok((addr, Some(hosts)))
}

/// 猜一个「手机该连的地址」：绑到 `0.0.0.0` 时本机可能有多个网卡，这里 connect 一个
/// 公网地址（UDP，不发包）让内核挑一条出口路由，得到最可能的局域网 IP。**只是提示**，
/// 多网卡/无默认路由时会不准（返回 `None`，日志里就写占位符）。
fn guess_lan_ip() -> Option<IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip())
}

/// Start the MCP server on a dedicated tokio runtime thread.  Fire-and-forget: the
/// thread ends when the app exits.
///
/// 监听地址 / 端口 / token / Host 白名单都由启动选项（CLI 或环境变量）解析后传入——
/// 这里不再自己读环境变量，避免出现「CLI 指定了但服务仍按 env 起」的双份真相。
pub fn spawn_server(app: AppHandle, config: ServerConfig) {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build MCP tokio runtime");
        runtime.block_on(serve(app, config));
    });
}

/// Build the axum router + listener and serve MCP until the process exits.
async fn serve(app: AppHandle, config: ServerConfig) {
    let token = config.token.filter(|token| !token.is_empty());
    let (bind, allowed_hosts) = match resolve_bind(&config.bind, &config.allow_hosts) {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("metatorio MCP server 未启动：{error}");
            return;
        }
    };

    let server_config = StreamableHttpServerConfig::default()
        // Stateless for every protocol version: each request gets a fresh
        // handler, shared state lives in the managed `AppState`.
        .with_legacy_session_mode(false)
        // Simple request/response tools reply as `application/json`.
        .with_json_response(true);
    let server_config = match &allowed_hosts {
        Some(hosts) => server_config.with_allowed_hosts(hosts.clone()),
        None => server_config.disable_allowed_hosts(),
    };

    let service = StreamableHttpService::new(
        move || Ok(MetatorioMcp { app: app.clone() }),
        LocalSessionManager::default().into(),
        server_config,
    );

    let router = Router::new()
        .nest_service(MCP_PATH, service)
        .layer(middleware::from_fn_with_state(token.clone(), require_token));

    let addr = SocketAddr::new(bind, config.port);
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
    // 手机等其它设备要连的地址、以及最容易踩的坑，直接打在启动日志里：这个端点不再
    // 只给本机用时，「手机上填什么」完全取决于它。
    if !bind.is_loopback() {
        let reachable = if bind.is_unspecified() {
            guess_lan_ip()
                .map(|ip| ip.to_string())
                .unwrap_or_else(|| "<本机局域网 IP>".to_string())
        } else {
            bind.to_string()
        };
        eprintln!(
            "  其它设备（手机/局域网）：http://{reachable}:{}{MCP_PATH}",
            config.port
        );
        match &allowed_hosts {
            Some(hosts) => eprintln!("  Host 白名单：{}", hosts.join(", ")),
            None => eprintln!(
                "  注意：绑定 {bind} 时 Host 校验已关闭（无法枚举本机地址），访问控制只靠 token"
            ),
        }
        if token.is_none() {
            eprintln!(
                "  危险：没有 token——这个端点能改文档、跑规划，暴露到局域网前请加 --mcp-token"
            );
        }
    }
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

    /// `auto_plan` 的消息序列：顺序与形状就是这个工具的契约——星球/品质在目标
    /// 之前（决定求解环境）、`best` 在剔除之前（否则剔除被整体替换覆盖）、
    /// 插件塔逐条 append（索引即序号）、最后必须触发 `solve: auto-plan`。
    #[test]
    fn auto_plan_body_messages_are_ordered_and_complete() {
        let params = AutoPlanParams {
            targets: vec![
                AutoPlanTarget {
                    item: "iron-plate".to_string(),
                    quality: None,
                    amount: 60.0,
                },
                AutoPlanTarget {
                    item: "copper-plate".to_string(),
                    quality: Some("legendary".to_string()),
                    amount: 30.0,
                },
            ],
            project_name: Some("p".to_string()),
            factory_name: Some("f".to_string()),
            planet: Some("vulcanus".to_string()),
            major_quality: Some("legendary".to_string()),
            modules: Some(AutoPlanModules {
                best: true,
                quality: None,
                exclude: vec![IdWithQuality::new("efficiency-module-3", "normal")],
            }),
            beacons: vec![AutoPlanBeacon {
                beacon: IdWithQuality::new("beacon", "legendary"),
                count: Some(2),
                share: Some(8.0),
                modules: vec![AutoPlanBeaconModule {
                    module: IdWithQuality::new("speed-module-3", "legendary"),
                    count: Some(2),
                }],
            }],
            external_inputs: vec![AutoPlanExternalInput {
                flow: DualVar::Item(IdWithQuality::new("iron-plate", "normal")),
                penalty: None,
            }],
            context_id: Some("ctx".to_string()),
            request_id: None,
        };
        let targets = vec![
            (IdWithQuality::new("iron-plate", "normal"), 60.0),
            (IdWithQuality::new("copper-plate", "legendary"), 30.0),
        ];
        let messages = auto_plan_body_messages(&params, ProjectId(7), FactoryId(9), &targets);

        // 星球 → 主品质 → 两个目标 → 外部输入 → best → 剔除 → 插件塔×2 → 触发。
        assert_eq!(messages.len(), 10, "{messages:#?}");
        let encoded: Vec<serde_json::Value> = messages
            .iter()
            .map(|message| serde_json::to_value(message).unwrap())
            .collect();
        // 工厂由工具先建（要拿 id），本序列**绝不能再建工厂**：否则一个项目里会多出
        // 一个空模板工厂（实测踩到）。这条断言就是那个回归的守卫。
        assert!(
            !encoded
                .iter()
                .any(|message| message["action"]["action"].get("add-factory").is_some()),
            "auto_plan 的配置序列里不该再有 add-factory：{encoded:#?}"
        );
        assert_eq!(
            encoded[0]["action"]["action"]["context"]["set-planet"]["planet"],
            "vulcanus"
        );
        assert_eq!(
            encoded[1]["action"]["action"]["context"]["set-major-quality"]["quality"],
            "legendary"
        );
        // 目标是「物品 + 品质 + 每秒速率」，两个目标各一条消息。
        assert_eq!(
            encoded[2]["action"]["action"]["flow"]["add-to-target"]["flow"]["Item"]["id"],
            "iron-plate"
        );
        assert_eq!(
            encoded[2]["action"]["action"]["flow"]["add-to-target"]["amount"],
            serde_json::json!(60.0)
        );
        assert_eq!(
            encoded[3]["action"]["action"]["flow"]["add-to-target"]["flow"]["Item"]["quality"],
            "legendary"
        );
        // 外部输入：penalty 省略时默认 1.0（严格供给下靠它声明原料来源）。
        assert_eq!(
            encoded[4]["action"]["action"]["flow"]["add-to-external-input"]["penalty"],
            serde_json::json!(1.0)
        );
        // best 用工厂主品质（modules.quality 省略），且在剔除之前。
        assert_eq!(
            encoded[5]["action"]["action"]["planning"]["use-best-modules"]["quality"],
            "legendary"
        );
        assert_eq!(
            encoded[6]["action"]["action"]["planning"]["remove-enumerated-module"]["module"]["id"],
            "efficiency-module-3"
        );
        // 插件塔：先 append 空方案，再把方案写进该索引（index = 0）。
        assert_eq!(
            encoded[7]["action"]["action"]["planning"],
            "add-enumerated-beacon"
        );
        let plan = &encoded[8]["action"]["action"]["planning"]["set-enumerated-beacon"];
        assert_eq!(plan["beacon"], serde_json::json!(0));
        let beacon = &plan["plan"]["module_config"]["beacons"][0];
        assert_eq!(beacon["beacon"]["id"], "beacon");
        assert_eq!(beacon["count"], serde_json::json!(2));
        assert_eq!(beacon["share"], serde_json::json!(8.0));
        assert_eq!(beacon["modules"][0][0]["id"], "speed-module-3");
        assert_eq!(beacon["modules"][0][1], serde_json::json!(2));
        // 最后一条必须是触发（否则整个入口等于没跑规划）。
        assert_eq!(encoded[9]["action"]["action"]["solve"], "auto-plan");
    }

    /// 目标名解析：id 与本地化名都行、分隔符不敏感；**不猜**——指向配方或拼错时
    /// 报错并给出候选（错误的名字能过校验，却会让计划悄悄跑偏）。
    #[test]
    fn auto_plan_targets_resolve_strictly_with_hints() {
        let entry = |kind: &str, name: &str, localized: &str| crate::IndexEntry {
            kind: kind.to_string(),
            name: name.to_string(),
            localized_name: localized.to_string(),
            group: String::new(),
            subgroup: String::new(),
            icon_type: String::new(),
            module_slots: None,
            categories: Vec::new(),
            fuel_category: String::new(),
            fuel_value_j: None,
            technology_max_level: None,
            technology_base_level: 0,
        };
        let index = crate::CatalogIndex {
            context_id: "c".to_string(),
            qualities: vec!["normal".to_string()],
            entries: vec![
                entry("item", "processing-unit", "处理器"),
                entry("recipe", "processing-unit", "处理器"),
                entry("item", "iron-plate", "铁板"),
            ],
        };
        let target = |item: &str, amount: f64| AutoPlanTarget {
            item: item.to_string(),
            quality: None,
            amount,
        };

        // id / 本地化名 / 分隔符变体都能解析到物品原型。
        for name in ["iron-plate", "铁板", "IRON_PLATE", "iron plate"] {
            let resolved = resolve_auto_plan_targets(&index, &[target(name, 1.0)]).expect(name);
            assert_eq!(resolved[0].0.id, "iron-plate");
            assert_eq!(resolved[0].0.quality, "normal");
            assert_eq!(resolved[0].1, 1.0);
        }
        // 同名跨 item/recipe 时取物品（目标只能是物品）。
        let resolved =
            resolve_auto_plan_targets(&index, &[target("processing unit", 5.0)]).unwrap();
        assert_eq!(resolved[0].0.id, "processing-unit");
        // 品质原样带上。
        let resolved = resolve_auto_plan_targets(
            &index,
            &[AutoPlanTarget {
                item: "铁板".to_string(),
                quality: Some("legendary".to_string()),
                amount: 2.0,
            }],
        )
        .unwrap();
        assert_eq!(resolved[0].0.quality, "legendary");
        // 拼错 → 报错并附错拼候选，不猜。
        let error = resolve_auto_plan_targets(&index, &[target("iron-plte", 1.0)]).unwrap_err();
        assert!(error.contains("iron-plate"), "{error}");
        // 完全不认识 → 提示去查名字。
        let error = resolve_auto_plan_targets(&index, &[target("nonsense-xyz", 1.0)]).unwrap_err();
        assert!(error.contains("localized_names"), "{error}");
        // amount 必须是正数。
        let error = resolve_auto_plan_targets(&index, &[target("iron-plate", 0.0)]).unwrap_err();
        assert!(error.contains("正数"), "{error}");
    }

    /// `auto_plan` 的手写名字必须在**建任何东西之前**被挡住：两类实测过的静默失败
    /// ——`remove-enumerated-module` 对不存在的插件是 no-op（`retain` 找不到就报
    /// 「无变化」），`add-to-external-input` 更把不存在的物品**原样写进文档**、自动规划
    /// 照常报「成功」。两者都会让调用方以为配置生效了，所以这里必须报错并给候选。
    #[test]
    fn auto_plan_rejects_unknown_names_before_creating_anything() {
        let entry = |kind: &str, name: &str, localized: &str| crate::IndexEntry {
            kind: kind.to_string(),
            name: name.to_string(),
            localized_name: localized.to_string(),
            group: String::new(),
            subgroup: String::new(),
            icon_type: String::new(),
            module_slots: None,
            categories: Vec::new(),
            fuel_category: String::new(),
            fuel_value_j: None,
            technology_max_level: None,
            technology_base_level: 0,
        };
        let index = crate::CatalogIndex {
            context_id: "c".to_string(),
            qualities: vec!["normal".to_string()],
            entries: vec![
                entry("item", "iron-plate", "铁板"),
                entry("module", "speed-module-3", "速度插件 3"),
                entry("beacon", "beacon", "插件塔"),
                entry("fluid", "water", "水"),
            ],
        };
        let module = |id: &str| IdWithQuality::new(id, "normal");
        let params = |modules: Option<AutoPlanModules>,
                      beacons: Vec<AutoPlanBeacon>,
                      inputs: Vec<AutoPlanExternalInput>| AutoPlanParams {
            targets: vec![AutoPlanTarget {
                item: "iron-plate".to_string(),
                quality: None,
                amount: 1.0,
            }],
            project_name: None,
            factory_name: None,
            planet: None,
            major_quality: None,
            modules,
            beacons,
            external_inputs: inputs,
            context_id: None,
            request_id: None,
        };
        let fluid = |name: &str| DualVar::Fluid {
            name: name.to_string(),
            temperature: [15, 15],
        };
        let exclude = |ids: &[&str]| {
            Some(AutoPlanModules {
                best: false,
                quality: None,
                exclude: ids.iter().map(|id| module(id)).collect(),
            })
        };

        // 合法名字全部通过：包括「索引里的模块既是 item 也是 module」的两种 kind，
        // 以及没有原型的虚拟流（电）。
        assert!(validate_auto_plan_names(
            &index,
            &params(
                exclude(&["speed-module-3"]),
                vec![AutoPlanBeacon {
                    beacon: module("beacon"),
                    count: None,
                    share: None,
                    modules: vec![AutoPlanBeaconModule {
                        module: module("speed-module-3"),
                        count: None,
                    }],
                }],
                vec![
                    AutoPlanExternalInput {
                        flow: DualVar::Item(module("iron-plate")),
                        penalty: None,
                    },
                    AutoPlanExternalInput {
                        flow: fluid("water"),
                        penalty: None,
                    },
                    AutoPlanExternalInput {
                        flow: DualVar::Electricity,
                        penalty: None,
                    },
                ],
            )
        )
        .is_ok());

        // 剔除一个不存在的插件 → 报错，并把正确名字当候选给出来。
        let error = validate_auto_plan_names(
            &index,
            &params(exclude(&["speed-module-99"]), Vec::new(), Vec::new()),
        )
        .unwrap_err();
        assert!(error.contains("speed-module-3"), "{error}");
        assert!(error.contains("要剔除的插件"), "{error}");
        // 外部输入写错物品 → 报错（否则垃圾会被原样写进文档）。
        let error = validate_auto_plan_names(
            &index,
            &params(
                None,
                Vec::new(),
                vec![AutoPlanExternalInput {
                    flow: DualVar::Item(module("iron-plte")),
                    penalty: None,
                }],
            ),
        )
        .unwrap_err();
        assert!(error.contains("iron-plate"), "{error}");
        assert!(error.contains("外部输入物品"), "{error}");
        // 外部输入写错流体。
        let error = validate_auto_plan_names(
            &index,
            &params(
                None,
                Vec::new(),
                vec![AutoPlanExternalInput {
                    flow: fluid("watter"),
                    penalty: None,
                }],
            ),
        )
        .unwrap_err();
        assert!(error.contains("water"), "{error}");
        // 插件塔写错。
        let error = validate_auto_plan_names(
            &index,
            &params(
                None,
                vec![AutoPlanBeacon {
                    beacon: module("beacon-99"),
                    count: None,
                    share: None,
                    modules: Vec::new(),
                }],
                Vec::new(),
            ),
        )
        .unwrap_err();
        assert!(error.contains("插件塔"), "{error}");
        // 完全不着边际的名字 → 提示去查名字（不许猜一个相近的原型）。
        let error = validate_auto_plan_names(
            &index,
            &params(exclude(&["zzzzzzzz"]), Vec::new(), Vec::new()),
        )
        .unwrap_err();
        assert!(error.contains("localized_names"), "{error}");
    }

    /// `--mcp-bind` 解析：默认/`localhost` → 回环；本机 IP → 该 IP 进 Host 白名单；
    /// `0.0.0.0` → 无从枚举地址，返回 `None`（关闭 Host 校验，启动日志会写明）；
    /// 非法地址必须报错，而不是静默退化成回环。
    #[test]
    fn bind_parsing_is_explicit_about_hosts_and_wildcards() {
        // 默认：回环，白名单只含回环名。
        let (addr, hosts) = resolve_bind("127.0.0.1", &[]).unwrap();
        assert!(addr.is_loopback());
        let hosts = hosts.expect("回环也要给白名单");
        assert!(hosts.contains(&"127.0.0.1".to_string()));
        assert!(hosts.contains(&"localhost".to_string()));

        // localhost 归一化成 127.0.0.1。
        assert_eq!(
            resolve_bind("localhost", &[]).unwrap().0,
            resolve_bind("127.0.0.1", &[]).unwrap().0
        );

        // 具体局域网 IP：该 IP 必须在白名单里（手机用 http://<IP>:port 访问）。
        let (addr, hosts) = resolve_bind(" 192.168.1.23 ", &[]).unwrap();
        assert_eq!(addr.to_string(), "192.168.1.23");
        let hosts = hosts.unwrap();
        assert!(hosts.contains(&"192.168.1.23".to_string()));
        // 额外 Host（主机名/mDNS）会追加，且不重复。
        let (_, hosts) = resolve_bind(
            "192.168.1.23",
            &["mirac-pc.local".to_string(), " 192.168.1.23 ".to_string()],
        )
        .unwrap();
        let hosts = hosts.unwrap();
        assert!(hosts.contains(&"mirac-pc.local".to_string()));
        assert_eq!(
            hosts.iter().filter(|host| *host == "192.168.1.23").count(),
            1
        );

        // 所有网卡：白名单为 None = 关闭 Host 校验（日志里会说明）。
        assert!(resolve_bind("0.0.0.0", &[]).unwrap().1.is_none());
        assert!(resolve_bind("::", &[]).unwrap().1.is_none());

        // 非法地址 / 空串：报错带上开关名，便于排查。
        assert!(resolve_bind("not-an-ip", &[])
            .unwrap_err()
            .contains("mcp-bind"));
        assert!(resolve_bind("  ", &[]).unwrap_err().contains("mcp-bind"));
        // 主机名不解析（只接受 IP 字面量与 localhost），避免依赖 DNS 结果。
        assert!(resolve_bind("mirac-pc", &[]).is_err());
    }

    /// 展开结果是**带符号**的一串流（正=产出、负=消耗），直接给 LLM 看 `-2.0` 容易
    /// 被读成「产出 2」。这里按数组名承载方向、数值一律正数。
    #[test]
    fn mechanic_flow_splits_into_positive_inputs_and_outputs() {
        let iron = DualVar::Item(IdWithQuality::new("iron-plate", "normal"));
        let gear = DualVar::Item(IdWithQuality::new("iron-gear-wheel", "normal"));
        let water = DualVar::Fluid {
            name: "water".to_string(),
            temperature: [15, 15],
        };

        let (inputs, outputs) = split_mechanic_flow(&[
            (iron.clone(), -2.0),
            (gear.clone(), 1.0),
            (water.clone(), -0.5),
        ]);
        assert_eq!(inputs.len(), 2);
        assert_eq!(outputs.len(), 1);
        // 消耗是正数（方向由 inputs 数组承载）。
        assert_eq!(inputs[0], (iron, 2.0));
        assert_eq!(inputs[1], (water, 0.5));
        assert_eq!(outputs[0], (gear, 1.0));

        // 只有消耗（例如采矿机吃电）或只有产出（例如太阳能）都不能丢。
        let (inputs, outputs) = split_mechanic_flow(&[(DualVar::Electricity, -0.09)]);
        assert_eq!(inputs.len(), 1);
        assert!(outputs.is_empty());
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
