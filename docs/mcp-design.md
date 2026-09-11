# MCP 工具 —— 设计决策（Phase 1）

## 决策 1（核心）：**合并进本体，不单独拆 bin —— 推荐**

**结论：MCP 服务器作为 Tauri 主程序内的一个**后台任务**（本地 Streamable-HTTP 端点），而不是独立的 `metatorio-mcp` 可执行文件。**

理由（正面回应你的两点顾虑）：

**① 更新问题 —— 合并即消除**
- 独立 bin 会让"自更新"要同时维护**两条产物链**：Tauri 应用（setup.exe / AppImage + `tauri-plugin-updater`，我们已经接好）+ headless MCP 二进制。两条更新语义、两个版本号、两个 `latest.json` 条目。
- **合并后只有一份产物**：GUI + MCP 端点都在同一个二进制里，`tauri-plugin-updater` 照常更新，**无需任何双版本逻辑**。这直接把你担心的"自更新命令要同时考虑 tauri 与 headless"消掉了。

**② 并存操作 —— 合并才可行**
- 外部 agent 框架和 GUI 用户**同时操作同一批项目**，前提是**共享同一个运行时状态**。
- 本体已经 `AppState { runtime: Mutex<Runtime>, … }`。MCP 工具调用 → `Mutex<Runtime>` → `runtime.dispatch(AppMessage)`（复用已有的 `RuntimeCommand` 包装路径），与 GUI 自身的 dispatch **串行化到同一把锁**。**同一个 Runtime 实例 → 同一份项目/工厂/求解状态** → 实时并存。
- 独立 bin 则每个进程一份内存状态，只能靠"读写同一份工程文件"间接同步，**做不到实时同界面协同**。

## 决策 2：MCP 传输与生命周期
- **传输**：`streamable-http`，绑定 `127.0.0.1:<port>`（仅本机），**需要 token**（见安全）。
- **生命周期**：随应用启动作为后台任务/线程拉起；应用关闭即停。Tauri v2 有 `tauri-plugin` 机制，也可以做成一个可开关的 setting（默认关，或默认开+token）。
- 若将来真要 headless（无窗口启动、stdio），**仍是同一二进制**：加一个启动 flag（如 `--mcp-stdio`，抑制窗口、改走 stdin/out）。**产物不变 → 更新逻辑更不用改**。

## 决策 3：并存架构可行性
- 现状已满足肉身：`Mutex<Runtime>` 天然线程安全，MCP HTTP handler 在别的线程上锁它即可，无数据竞争。
- **需要新增的小能力**：
  1. 一条 **"状态已变更"广播事件**（`document/changed`）。目前 GUI 只在自己 `send()` 后 `refresh()`；MCP 外部写入后不会自动刷新 GUI。要在 MCP 写路径上 `emit` 变更事件，GUI 收到后重拉文档 → 两边视图实时一致。
  2. MCP 命令集：与 GUI 同源的 `AppMessage` 子集（列项目/工厂、读目标/机制/求解、重算/自动规划、改目标/机制、载入上下文等）。

## 决策 4：更新
- 合并后 = 现有 Tauri updater 更新整包（GUI + MCP 端点）。**无需 headless 单独更新**。

## 决策 5：安全
- 仅 `127.0.0.1`，配 **token**（配置或按会话生成），外部 agent 需带 token 调用；避免任何本机进程都能驱动规划器。
- 复用 MCP 客户端注意：DSH 接 MCP 时服务器命令是**沙箱外**代码；这里我们是**被动提供 MCP 服务**，方向相反，安全边界是"只信任持 token 的调用方"。

## 决策 6：MCP 工具面（Outline，Phase 2 细化）
- `list_projects` / `list_factories`
- `get_planning_state`（目标/表达式/外部输入/机制/求解状态）
- `recompute` / `auto_plan` / `cleanup`
- `add_target` / `set_target_amount` / `add_mechanic` / `set_recipe` / `set_machine` / `set_module`…
- `load_context`（载入游戏上下文）
- 即：**把 GUI 能做的规划操作，以 MCP 工具形式暴露**；内部全部走 `runtime.dispatch`。

## 开放问题 / 风险
- **并发语义**：外部 agent 与 GUI 同时改同一工厂时，最后一次 `dispatch` 胜出；需不需要"乐观锁/变更冲突提示"？（低保真可先"后写覆盖"，提示用户。）
- **长时求解阻塞**：`recompute` 在 MCP 调用里是同步的，会占住 `Mutex<Runtime>` 一段时间 → 期间 GUI 操作排队。需要评估：让 MCP 的求解走**异步**（类似 GUI 的 solving 事件），避免长时间锁死 GUI。
- **端口/复用**：固定端口 vs 动态端口；若被占用或需要多实例，需处理。
- **token 传递**：DSH 接 Streamable-HTTP 时 `headers` 放 `Authorization`，可与 DSH 的 mcp-client `headers` 字段直接对上。

## 已拍板（供 relay context 接力）
1. **合并进主二进制**（Streamable-HTTP 端点），**不单独拆 MCP bin** → 消除双产物更新问题。
2. 复用 `Mutex<Runtime>` 共享状态，新增 `document/changed` 广播事件以支持 GUI 与外部 agent **实时并存协同**。
3. 本地化 + token 鉴权；`--mcp-stdio` 可选 headless（仍是同一二进制）。
4. MCP 工具面 = GUI 规划操作的 `runtime.dispatch` 子集。
5. 待细化：并发冲突语义、长求解的异步化、端口/token 细节。

## 决策 7（MVP 落地，2026-xx）：**单 `dispatch` 万能工具先行**

Phase 2 工具面**不在一开始就做成离散的友好工具**，而是：

1. **MVP = 一个 `dispatch` 工具**，接受 `AppMessage`，转发给 `runtime.dispatch`，与 GUI 共用同一 `Mutex<Runtime>`。
   - 理由：`AppMessage` 已无缝对接调度核心，一个工具即覆盖整个动作集；不用为每种操作设计参数结构，永不失效。
   - **代价（已接受）**：`AppMessage` 是超大嵌套 enum，LLM 手写 JSON 易错。这是**确定性接口** + 收集 agent 使用反馈用的。
   - **采用 `JsonSchema` 派生（revised）**：`AppMessage` 用 `#[serde(tag="scope", content="action")]` 邻接标签。实测 schemars 1.2 能**精确还原**该形状（`{scope, action:{project, factory, action}}`），且外部标签 enum/内部标签/`[i32;2]` 数组/`Vec<(A,usize)>` 元组均正确。故为整个 `AppMessage` 传递闭包派生 `JsonSchema`（core：`IdWithQuality`/`DualVar`/`Fuel`/`ModuleConfig`/`BeaconConfig`/`Accessible`；runtime：id newtype + document 结构 + 全部 action 子枚举），工具参数直接 `message: AppMessage`，inputSchema 即真实消息 schema，替代 V1 的 `serde_json::Value` + 服务端 `from_value` 反序列化。

### 实现要点（已落地）

- **传输**：`rmcp 3.2.0` Streamable-HTTP（Tower service），挂到 axum，`/mcp` 路径。
- **生命周期 / 启动参数**：`spawn_server(app.handle().clone(), port, token)` 在 `run()` 的 `setup` **开头**启动（先起端点，再做可能数十 MB 的上下文恢复，agent 不必等）；独立 tokio 多线程 runtime 线程，绑定 `127.0.0.1:<port>`。参数由 bin 的 clap 解析后传入（`Options`）：`--mcp-port`（默认 8765）、`--mcp-token`、`--solve-timeout-ms`、`--headless`、`--no-mcp`，**同名环境变量作为回退**（`METATORIO_MCP_PORT` / `METATORIO_MCP_TOKEN` / `METATORIO_SOLVE_TIMEOUT_MS` / `METATORIO_HEADLESS` / `METATORIO_NO_MCP`），优先级 CLI > env > 默认。lib 不再自己读环境变量，避免「CLI 指定了但服务仍按 env 起」的双份真相。`--headless` 与 `--no-mcp` 互斥（那等于没有窗口也没有接口），启动时直接报错退出。
- **无头模式**：`--headless` 在 `Builder::build` 之前清空 `tauri.conf.json` 的窗口配置（窗口是事件循环首次迭代的 `setup` 阶段才建的），`setup` 照常执行 → 一个窗口都不建、事件循环常驻，MCP 端点在 `http://127.0.0.1:<port>/mcp`。此时没有 webview，因此 `icon`/`pick_*` 等 GUI 命令无人调用；GUI 与 headless 是同一个二进制（决策 1「同一进程共享 AppState」不变）。Linux 上 tauri 仍会初始化 GTK/X11（需 xvfb）。
- **token 鉴权**：axum middleware (`from_fn_with_state`)，`Authorization: Bearer <token>` 或裸 token；`--mcp-token` 未给（或为空）则**不鉴权**（仅 loopback 兜底）。rmcp 的 `StreamableHttpServerConfig` 默认只接受 loopback `Host`（DNS rebinding 防护）。
- **共享状态**：工具 handler 持有 `AppHandle`，经 `app.state::<AppState>()` 取 `Mutex<Runtime>`；`dispatch` 后对每个 `RuntimeCommand` 调 `execute_command`（同样会 `emit` solve-result 等事件，GUI 实时更新）。
- **co-op 广播**：`runtime.dispatch` 返回 `outcome.changed` 时，MCP 端 `emit("document-changed", revision)`；前端 store 订阅后调 `refresh()` 重拉文档快照 → **外部 agent + 用户在同一个界面实时并存协同**。
- **`execute_command` 改为返回 `Option<CommandEffect>`**：Recompute/AutoPlan/Cleanup 返回 `Some(effect)` 供 MCP 直接 surfacing solve 结果；其余命令返回 `None`；原有 Tauri emit 副作用全部保留（非破坏性）。

### 工具面（MVP）

| 工具 | 参数 | 说明 |
| --- | --- | --- |
| `dispatch` | `{ message: AppMessage, request_id? }` | 万能回退：转发任意规划动作，返回 `revision`/`changed`/`created`/`scheduled_commands`/`solve`（均结构化 JSON） |
| `get_planning_state` | `{ project?, factory?, recompute? }` | 读取：Omit `project` → 全文档；`project` → 单项目；`project`+`factory` → 单工厂；`recompute`（需 project+factory）时先求解并附带结构化结果 |
| `list_projects` | — | 项目索引（id/名称/上下文/工厂·机制·目标计数） |
| `list_factories` | `{ project }` | 单项目下的工厂索引（含目标清单） |
| `list_contexts` | — | 游戏数据上下文索引（id/名称/来源/是否已载入/哪个是激活的）。上下文是内容哈希缓存，agent 只能列举与切换，不能创建 |
| `list_prototypes` | `{ kind?, name_contains?, context_id? }` | 领域词表：该上下文里的物品/流体/配方/科技/机器/资源…（name、localized_name、group/subgroup、categories、燃料信息、插件槽）。返回 `total`/`matched`/`entries`，不做截断——用 `kind`/`name_contains` 收窄 |
| `suggest` | `{ flow, context_id? }` | 给定一条流，列出能产出/消耗它的候选机制（recipe/resource/item-fuel/generator，含 role）——「加机制」前的第一步 |

后续按 agent 真实使用反馈，再把常见需求从 `dispatch` 拆出更友好的专用工具（仍在同一 `dispatch` 路径之上）。

## 决策 8（反馈驱动修正，2026-xx）：**首个 agent 实证后的修正**

另一个 agent 实际调用 `mcp__metatorio__dispatch` 走通完整链路后给出高价值反馈，据此修正 V1：

### 已修正

1. **文档示例格式错误（P0-2）**：原工具 description 里 project/factory 示例写成 `{scope, project, factory, action}`，把 `project`/`factory` 放在 `scope` 同级——但 `AppMessage` 是**相邻标签**，所有字段须进 `action`。已改为 `{scope:"project", action:{project, action}}`。**根因**是 V1 用 `serde_json::Value` 手写描述，靠人肉确保格式正确 → **改用 `JsonSchema` 派生后，schema 自动对齐 serde 输出，此类错误不会再复发**。
2. **`solve` 返回 Rust `Debug` 字符串（P1-3）**：`format!("{effect:?}")` 不结构化、LLM 难解析。已改为把 `CommandEffect::Solve(result)` 的 `SolveResult` 序列化为 JSON。
3. **`scheduled_commands` 只返回数量（P1-4）**：agent 不知是哪几条副作用。已改为返回真实 `RuntimeCommand` 序列化数组。
4. **新建对象不知 id（P1-5）**：`dispatch` 只回 revision，读不到新分配的 project/factory/mechanic id。**仍待补**（方案见下），由 `get_planning_state` 承担读取职责或直接抽取新 id。

### 正交性原则（功能边界）

**Metatorio 的 MCP 工具只负责产出全量、结构化、可直接序列化的 JSON；「如何筛选/切片/摘要结果」不属于它的职责范围**——支持 MCP 的 agent 上下文里自有 JSON 处理工具来处理。因此：
- **不加入**结果截断、分页、字段裁剪、human-readable 摘要等逻辑（那是 agent 侧 JSON 工具的事）。
- 结果再长也接受，保持原样全量返回（如 `get_planning_state` 省略 `project` 时返回完整 `AppDocument`）。
- 前提：产出必须是**标准 JSON**（对象/数组，字段名稳定、可被既有 JSON 工具处理），而非 Rust `Debug` 字符串这类非结构化的东西——后者才是真正的缺陷（P1-3 已修）。

### 待补（按优先级）

- **读取工具**：`dispatch` 目前**纯写入、无自省**——agent 看不到当前文档、拿不到 id。~~方案：加一个 `get_planning_state` 读取工具~~ **已实现**：`get_planning_state(project?, factory?, recompute?)` 读 `runtime.state.document` 快照，一并解决 P0-1（只写不读）+ P1-5（id）+ P1-3（solve 结构化读取）。
- **长求异步化**：`recompute`/`auto_plan` 同步占住 `Mutex<Runtime>`，期间 GUI 排队。方案：投递后台任务 + 经 `document-changed`/solving 事件回报（原决策 47）。~~**待补**~~ **已完成**（2026-xx）：`dispatch` 改为「reducer 短临界区 → 逐条命令各自按需短锁」，求解/自动规划在锁外跑（`solve_jobs` 按 `(project, factory)` 单飞 + latest-wins + revision 戳）；自动规划/清理的回写走 reducer 并校验版本，MCP 端按 revision 变化补发 `document-changed`。详见 `docs/runtime-concurrency.md`。
- **领域词表**：~~`list_contexts`~~ / ~~`list_prototypes`~~ **已实现**（`list_contexts`、`list_prototypes`，后者支持 `kind` 与 `name_contains` 过滤，GUI 的 `catalog_index` 命令与它共用同一实现）；候选机制建议 **已实现**（`suggest`，与 GUI 的 `suggest` 命令共用 `suggest_for_flow`）。仍未工具化的是 `allowed_modules` / `implicit_sources` / `mechanic_flow` / `accessibility` / `productivity`（GUI 有命令，agent 暂时只能通过 `get_planning_state` 的求解结果间接观察）。
- **上下文可写**：上下文的切换/重命名/删除已从「只有 Tauri 命令」收敛为 `AppMessage`（`ApplicationAction::SetActiveContext` / `RenameContext` / `DeleteContext`），因此 agent 用 `dispatch` 即可操作；app 层 `activate_context` / `rename_registered_context` / `delete_registered_context` 是 GUI 与 agent 共用的单一实现。
- **工程文件可读写**：`open-project { path }` / `save-project { project }` / `save-project-as { project, path }` 现在是 GUI 的真实路径（Tauri 侧只保留文件对话框 `pick_project_file` / `pick_project_save_path` 与只读的 `project_save_path`），因此 agent 也能按路径打开/另存工程。显式保存走专用命令 `RuntimeCommand::SaveProject`：没有记忆路径时**报错**（提示先用 save-project-as），而自动落盘的 `Persist{path:None}` 对未保存过的新项目仍静默跳过——两者语义不同，不可混用。
- **未实现变体在 schema 里自述**：`request-suggestions` / `replace-from-location` 与更新三连的文档注释会进入 MCP inputSchema（测试 `unimplemented_variants_are_documented_in_the_schema` 守住这一点），agent 读 schema 即可知道它们目前一定失败，不必先浪费一次调用。同一轮删除了**废弃的 suggestion 会话**（`FactoryAction::Suggestion` / `SuggestionAction` / `SuggestionCandidate` 及其 `apply_suggestion`）：它对应「运行时持有建议状态」的旧设计，而真实能力早已由只读命令 `suggest` / `implicit_sources` + `mechanic-list` 消息提供，剩下的三条分支（`SetFilter` / `Dismiss` / `SelectMechanic`）全是静默 no-op，只会让调用方误以为会话模型存在。
- **`use-best-modules` 语义修正并实现**：旧签名 `UseBestModules { factory, mechanic }` 把 factory/mechanic 塞进一个**项目级**设置（`planning.enumerate_modules`）里，且 app 层从未实现。现改为 `UseBestModules { quality: String }`：项目级、**品质必填且不做推断**（品质是特殊维度——「解锁某品质」不等于「能大规模量产该品质的插件」，所以不能拿项目品质上限之类的东西当默认；GUI 传当前工厂的主品质）、候选按当前可达性过滤、同类别 tier 并列时取名字最小者（确定性 + 幂等），在 `Runtime::dispatch` 里解析后走 `finish`（revision/落盘/重解全部工厂）。GUI 的「使用最佳插件」按钮从「只读命令 + 可达性过滤 + N 条 add/remove 消息」收敛为**一条消息**；原 `best_modules` Tauri 命令与 `RuntimeCommand::UseBestModules`（占位）随之删除。
- **友好工具拆分**：`list_projects` / `add_target` / `set_target_amount` / `add_mechanic` / `set_recipe` / `set_machine` / `recompute` / `auto_plan` / `load_context`（原决策 6）。
- **并发冲突语义**（原决策 45）；**端口/多实例/token 细节**（原决策 48）。

## 待办 / 下阶段

- [x] 真实跑一次应用，用 DSH 客户端连 `http://127.0.0.1:8765/mcp`，验证 `dispatch` 工具可驱动规划、GUI 实时刷新。
- [x] 收集 agent 使用 AppMessage 的感受 → 修正：`JsonSchema` 派生（inputSchema 真实化）+ `solve`/`scheduled_commands` 结构化 + 修正文档示例。
- [x] **补读取工具**（`get_planning_state`）——P0-1/P1-5 的核心，MVP 目前最大的盲区。
- [ ] 采集使用反馈 → 抽离友好工具（`list_projects` / `get_planning_state` / `add_target` / `set_target_amount` / `add_mechanic` / `set_recipe` / `set_machine` / `recompute` / `auto_plan` / `load_context`）——对应原决策 6。（`list_projects` / `list_factories` / `list_contexts` 已落地。）
- [ ] 长时求解（`recompute`/`auto_plan`）走**异步**（类似 GUI 的 solving 事件），避免 MCP 调用期间占住 `Mutex<Runtime>` 导致 GUI 排队——原决策 47 的风险。→ **已完成**，见 `docs/runtime-concurrency.md`。
- [ ] 并发冲突语义（乐观锁/变更冲突提示）——原决策 45。
- [ ] 端口被占用 / 多实例 / token 传递细节——原决策 48。

## DSH 接入（验证用）

DSH 客户端配置见 DSH 仓库 `@deepseek-ai/dsh-mcp-client` 的 Streamable-HTTP 接法：命令（stdio）不用时，可改成 HTTP 端点 + `Authorization: Bearer <token>` 头部。本服务器**默认提供 HTTP 端点**，故 DSH 侧用 http transport 配置即可，工具名会带 `mcp__<serverName>__dispatch` 前缀。

要我把上面整理成一份**可写进仓库的设计稿**（比如 `docs/mcp-design.md`，含"决策理由"树，方便下轮 compact 接力），还是就以这段对话为准等 compact 自动节选？另外：是否需要我下轮从"工具面枚举 + 异步求解"继续（Phase 2 规划细化），还是先停在架构决策这层？