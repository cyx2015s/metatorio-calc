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

## 决策 5：安全（2026-xx 修订：默认仍只回环；要局域网/手机接入则非回环必须有 token）
- **默认只监听 `127.0.0.1`**（`--mcp-bind` 的默认值）：这个端点能建项目、改目标、跑规划，默认不该被同一网段里任何设备碰到。
- 要让手机等其它设备接入：`--mcp-bind <本机局域网 IP>`（或 `0.0.0.0`）。**非回环绑定必须同时提供 `--mcp-token`，否则直接拒绝启动**（bin 的 `validate` 报错退出，而不是警告后照跑）——家用网段里任何设备都能扫到开放端口。
- 鉴权：`Authorization: Bearer <token>`（或裸 token）。回环 + 无 token 仍是允许的（本机自用）。
- **Host 白名单**：rmcp 的 Streamable HTTP 默认只接受回环 `Host`（防 DNS rebinding）。绑定具体 IP 时自动把该 IP 加进白名单（手机用 `http://<IP>:<port>/mcp` 即可）；用主机名/mDNS 名访问再加 `--mcp-allow-host <name>`；绑 `0.0.0.0`/`::` 时无法枚举本机地址，白名单**关闭**（启动日志会写明），此时访问控制只剩 token。实测：错误 `Host` → `403 Forbidden: Host header is not allowed`。
- 启动日志直接打印「其它设备该用的 URL」+ 当前白名单（绑 `0.0.0.0` 时用一次 UDP connect 猜一个局域网 IP），减少「手机上到底填什么」的来回试。
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
| `auto_plan` | `{ targets[{item,quality?,amount}], project_name?, factory_name?, planet?, major_quality?, modules?{best,quality?,exclude[]}, beacons?[{beacon,count?,share?,modules?}], external_inputs?[{flow,penalty?}], context_id?, request_id? }` | **一站式入口**：一次调用建项目/工厂、配好星球/主品质/目标/外部输入/插件策略/插件塔方案，然后**异步**开跑自动规划并**立刻返回**（`project`/`factory` id + `auto_plan.status=running` + `poll` 提示）。稍后 `get_planning_state {project, factory}` 读 `auto_plan.status`（running/done/failed）与结果 |
| `dispatch` | `{ message: AppMessage, request_id?, limit?, offset? }` | 万能回退：转发任意规划动作，返回 `revision`/`changed`/`created`/`scheduled_commands`/`solve`（均结构化 JSON）。`solve` 的 `mechanics`/`flows` 按 `limit`/`offset` 分页（默认 50、上限 1000）。**同步**等待求解完成 |
| `get_planning_state` | `{ project?, factory?, mechanic?, recompute?, limit?, offset? }` | **逐层读取**：无 `project` → 项目索引（原 `list_projects`）；`project` → 设置/规划偏好 + 工厂索引（原 `list_factories`，`mechanics` 只是数量）；`project`+`factory` → 该工厂文档（机制/目标/目标表达式/外部输入分页）+ 该工厂的 `auto_plan` 状态；**`project`+`factory`+`mechanic` → 聚焦到这一个机制**（`config` = 类型/机器/配方/插件/插件塔/燃料，加 `inputs`/`outputs` = **系数 = 1 时每秒的消耗与产出**，已含机器速度、插件与插件塔效果——与 GUI 机制卡同一份展开，所以 agent 不必从机制名去猜它到底在做什么）；`recompute`（需 project+factory）时附带同步求解结果（同样分页）。所有响应带 `page.totals` / `page.truncated` |
| `list_contexts` | — | 游戏数据上下文索引（id/名称/来源/是否已载入/哪个是激活的）。上下文是内容哈希缓存，agent 只能列举与切换，不能创建 |
| `list_prototypes` | `{ kind?, name_contains?, context_id?, limit?, offset? }` | 领域词表：该上下文里的物品/流体/配方/科技/机器/资源…（name、localized_name、group/subgroup、categories、燃料信息、插件槽）。`entries` 按 `limit`（默认 50、上限 1000）/`offset` 分页，带 `total`/`matched`/`returned`/`page` |
| `localized_names` | `{ queries[], kind?, limit_per_query?, context_id? }` | **名字 ↔ 本地化名互查**：`queries` 可混用原型 id（`iron-gear-wheel`）与玩家口述的本地化名（`铁齿轮`）。匹配时**忽略 `-` / `_` / 空白**（`processing unit`＝`processing_unit`＝`PROCESSING-UNIT`＝`processing-unit`），先精确命中（id、本地化名各一轮），再按「本地化名前缀 → id 前缀 → 本地化名子串 → id 子串」给模糊命中，逐条带 `matched_by`；精确与模糊都为空时再给**错拼候选**（编辑距离，相邻换位算 1 步；**按字符**算长度并对中文放宽到 2 个字符——`铁版` → `铁板` 是群里最典型的一幕），其中 `typo_suggestion` 只在「最佳距离上**名字唯一**」时非空（同名跨 item/recipe 不算歧义，kind 由调用方按上下文选）——几个名字同样接近（`processing-unit-2` 与 `-3`）时返回 null，让人确认；中文两字名常有十几个等距候选，此时 `typo` 按 `limit_per_query` 截断、`typo_matched` 给真实条数，建议交给人工/上层判断 |

工具面原则：**权威入口是 `auto_plan`**（从目标反推整条链）；`dispatch` 是覆盖全部消息的逃生通道；其余工具只服务「读」与「名字解析」。曾经的 `suggest`（列出某条流的候选机制）**已删除**——它会把 agent 引向「一个个配方手工拼装」的错误心智（正确做法是给目标让规划器枚举），而且与 `auto_plan` 的能力重叠。工具描述与参数说明**一律用中文**：垂直领域的术语（物品名、品质、插件塔）本来就是中文，中文描述比英文更准、也少一层翻译损耗（群内反馈）。

**报错也一律中文**（2026-xx）：runtime 的 `RuntimeError`（找不到项目/工厂/机制/目标、下标越界、重复 id、数值校验、`品质 X 不存在于当前游戏上下文`）、`validate.rs` 的原型名校验、app 层命令错误、MCP 工具的 `invalid_params`、启动日志与 dump 解析错误全部改成中文。实测（headless + 原生 MCP）：`找不到项目 999`、`配方 not-a-real-recipe 不存在于当前游戏上下文`、`重复的目标 id`、`插件塔 not-a-real-beacon 不存在于当前游戏上下文`、`机制 999 不在工厂 2 里：先 get_planning_state {project, factory} 读 mechanics 列表拿 id`。

> 唯一的例外是**框架级**报错：rmcp 反序列化工具参数失败时给出的 `failed to deserialize parameters: missing field ...` 由 serde 生成，要翻它得换掉 rmcp 的参数提取器，收益不值；agent 只在**参数形状写错**时才会看到它（语义错误全都走我们自己的中文校验）。

后续按 agent 真实使用反馈，再把常见需求从 `dispatch` 拆出更友好的专用工具（仍在同一 `dispatch` 路径之上）。

## 决策 8（反馈驱动修正，2026-xx）：**首个 agent 实证后的修正**

另一个 agent 实际调用 `mcp__metatorio__dispatch` 走通完整链路后给出高价值反馈，据此修正 V1：

### 已修正

1. **文档示例格式错误（P0-2）**：原工具 description 里 project/factory 示例写成 `{scope, project, factory, action}`，把 `project`/`factory` 放在 `scope` 同级——但 `AppMessage` 是**相邻标签**，所有字段须进 `action`。已改为 `{scope:"project", action:{project, action}}`。**根因**是 V1 用 `serde_json::Value` 手写描述，靠人肉确保格式正确 → **改用 `JsonSchema` 派生后，schema 自动对齐 serde 输出，此类错误不会再复发**。
2. **`solve` 返回 Rust `Debug` 字符串（P1-3）**：`format!("{effect:?}")` 不结构化、LLM 难解析。已改为把 `CommandEffect::Solve(result)` 的 `SolveResult` 序列化为 JSON。
3. **`scheduled_commands` 只返回数量（P1-4）**：agent 不知是哪几条副作用。已改为返回真实 `RuntimeCommand` 序列化数组。
4. **新建对象不知 id（P1-5）**：`dispatch` 只回 revision，读不到新分配的 project/factory/mechanic id。~~**仍待补**~~ **已补**：`DispatchResult` 增加 `created`（本次新建的 project/factory/mechanic id），`dispatch` 与 `auto_plan` 都直接回；读取职责另由 `get_planning_state` 承担。

### 输出有界原则（功能边界，2026-xx 修正）

~~原「正交性原则」：工具只产出全量 JSON，截断/分页是 agent 侧 JSON 工具的事。~~ **已被实战推翻**：群友实测反馈——「现在截断完全依赖我的 bash 返回给你兜底了，不然一条查询语句直接上下文爆炸」。真实量级：py 上下文目录索引 17757 条、一次自动规划写出 757 条机制，`get_planning_state` 省略 `project` 时会把每层工厂的每条机制一起倒出来。因此改为：

- **每个返回集合都必须有上界**：`limit`（默认 50、上限 1000）+ `offset`，工具自己保证，不依赖调用方兜底。
- **截断必须自证**：响应带 `page: { offset, limit, totals, truncated }`（`totals` 是截断前总数，`truncated` 列出被截断的集合名），被截断的集合另附 `<key>_hint` 说明如何收窄——**绝不静默丢数据**。
- **逐层只给下一层索引**：`get_planning_state` 无 `project` → 项目索引；给 `project` → 设置/规划偏好 + 工厂索引；给 `project`+`factory` → 该工厂文档（重复集合分页）。要看机制明细就必须指名到工厂，任何一次调用的返回量都由 `limit` 决定。
- 仍然坚持的前提：产出必须是**标准 JSON**（对象/数组、字段名稳定、可被 agent 侧 JSON 工具处理），而不是 Rust `Debug` 字符串这类非结构化文本。
- 不做的：按字段裁剪（`fields` 选择）——多一个心智负担，收益有限；agent 侧 JSON 工具更适合做这件事。

### 待补（按优先级）

- **读取工具**：`dispatch` 目前**纯写入、无自省**——agent 看不到当前文档、拿不到 id。~~方案：加一个 `get_planning_state` 读取工具~~ **已实现**：`get_planning_state(project?, factory?, recompute?)` 读 `runtime.state.document` 快照，一并解决 P0-1（只写不读）+ P1-5（id）+ P1-3（solve 结构化读取）。
- **长求异步化**：`recompute`/`auto_plan` 同步占住 `Mutex<Runtime>`，期间 GUI 排队。方案：投递后台任务 + 经 `document-changed`/solving 事件回报（原决策 47）。~~**待补**~~ **已完成**（2026-xx）：`dispatch` 改为「reducer 短临界区 → 逐条命令各自按需短锁」，求解/自动规划在锁外跑（`solve_jobs` 按 `(project, factory)` 单飞 + latest-wins + revision 戳）；自动规划/清理的回写走 reducer 并校验版本，MCP 端按 revision 变化补发 `document-changed`。详见 `docs/runtime-concurrency.md`。
- **领域词表**：~~`list_contexts`~~ / ~~`list_prototypes`~~ **已实现**（`list_contexts`、`list_prototypes`，后者支持 `kind` 与 `name_contains` 过滤，GUI 的 `catalog_index` 命令与它共用同一实现）；~~候选机制建议（`suggest`）~~ **已删除**（见上文工具面原则：把 agent 引向手工拼配方，且与 `auto_plan` 重叠；GUI 侧的同名命令仍保留给「建议」面板）；**名字解析已实现**（`localized_names`：id → 本地化名用于向群内汇报，口述名 → id 用于落成消息；匹配/排序口径抽成纯函数 `resolve_index_entry`，与 `list_prototypes` 共用同一份目录索引）。仍未工具化的是 `allowed_modules` / `implicit_sources` / `accessibility` / `productivity` / `solar_balance`（GUI 有命令，agent 暂时只能通过 `get_planning_state` 的求解结果间接观察）。
- **机制级读取（`mechanic_flow`，2026-xx）**：机制在文档里只有 machine/recipe/modules 这些**零件**，agent 光看机制名（甚至看零件）也推不出「一个系数到底在消耗什么、产出什么」——机器速度、插件、插件塔都参与之后更是如此。因此把 GUI 的机制卡数据（`mechanic_flow` 命令：系数 = 1 时的每秒产/耗）按**下一层**接进 `get_planning_state {project, factory, mechanic}`：`config` 给零件、`inputs`/`outputs` 给正数化的消耗与产出（方向由数组名承载，不再让 LLM 去读 `-2.0` 的符号）。计算路径与 GUI **同一份实现**（`crate::mechanic_flow_for`），避免「界面上显示的」和「agent 读到的」是两套算法。实测（vanilla-2.1）：钢炉炼铁板 → 0.625 铁矿石/秒 + 化学燃料 → 0.625 铁板/秒；蒸汽轮机 500°C → 60 蒸汽/秒 → 5.82 MW；3 邻核反应堆 → 40 MW 燃料 → 160 MW 热；同一条铀燃料电池配方带 4×产能插件 3 时，输入降到 0.5/0.05/0.95、电耗升到 1.5875 MW——**插件效果如实反映在数字里**。
- **上下文可写**：上下文的切换/重命名/删除已从「只有 Tauri 命令」收敛为 `AppMessage`（`ApplicationAction::SetActiveContext` / `RenameContext` / `DeleteContext`），因此 agent 用 `dispatch` 即可操作；app 层 `activate_context` / `rename_registered_context` / `delete_registered_context` 是 GUI 与 agent 共用的单一实现。
- **工程文件可读写**：`open-project { path }` / `save-project { project }` / `save-project-as { project, path }` 现在是 GUI 的真实路径（Tauri 侧只保留文件对话框 `pick_project_file` / `pick_project_save_path` 与只读的 `project_save_path`），因此 agent 也能按路径打开/另存工程。显式保存走专用命令 `RuntimeCommand::SaveProject`：没有记忆路径时**报错**（提示先用 save-project-as），而自动落盘的 `Persist{path:None}` 对未保存过的新项目仍静默跳过——两者语义不同，不可混用。
- **未实现变体在 schema 里自述**：`request-suggestions` / `replace-from-location` 与更新三连的文档注释会进入 MCP inputSchema（测试 `unimplemented_variants_are_documented_in_the_schema` 守住这一点），agent 读 schema 即可知道它们目前一定失败，不必先浪费一次调用。同一轮删除了**废弃的 suggestion 会话**（`FactoryAction::Suggestion` / `SuggestionAction` / `SuggestionCandidate` 及其 `apply_suggestion`）：它对应「运行时持有建议状态」的旧设计，而真实能力早已由只读命令 `suggest` / `implicit_sources` + `mechanic-list` 消息提供，剩下的三条分支（`SetFilter` / `Dismiss` / `SelectMechanic`）全是静默 no-op，只会让调用方误以为会话模型存在。
- **`use-best-modules` 语义修正并实现**：旧签名 `UseBestModules { factory, mechanic }` 把 factory/mechanic 塞进一个**项目级**设置（`planning.enumerate_modules`）里，且 app 层从未实现。现改为 `UseBestModules { quality: String }`：项目级、**品质必填且不做推断**（品质是特殊维度——「解锁某品质」不等于「能大规模量产该品质的插件」，所以不能拿项目品质上限之类的东西当默认；GUI 传当前工厂的主品质）、候选按当前可达性过滤、同类别 tier 并列时取名字最小者（确定性 + 幂等），在 `Runtime::dispatch` 里解析后走 `finish`（revision/落盘/重解全部工厂）。GUI 的「使用最佳插件」按钮从「只读命令 + 可达性过滤 + N 条 add/remove 消息」收敛为**一条消息**；原 `best_modules` Tauri 命令与 `RuntimeCommand::UseBestModules`（占位）随之删除。
- **友好工具拆分**：`auto_plan`（一站式入口，见下）已落地；其余候选（`add_target` / `set_target_amount` / `set_recipe` / `set_machine` / `load_context`）暂不拆——`dispatch` 已覆盖且 schema 就是真实协议。**同时删掉了 `list_projects` / `list_factories`**：它们的载荷（项目索引 / 工厂索引）现在正是 `get_planning_state` 的无 `project` / 给 `project` 两层的返回值，留着只是重复的工具面（群里「工具在精不在多」）。
- **`auto_plan` 一站式入口（2026-xx，群内 bot 实测驱动）**：群里 bot 每次指挥都要手拼 `dispatch` 序列（建项目 → 建工厂 → 星球 → 主品质 → 目标 → 外部输入 → 插件策略 → 插件塔 → solve），既费人又费 AI，还容易漏配严格供给。现在一条消息搞定，参数只暴露实测高频项（目标物品×品质×速率、星球、主品质、最佳/排除插件、插件塔方案、外部输入），并按「星球/品质 → 目标 → 外部输入 → best → 剔除 → 插件塔 → 触发」的固定顺序执行。
  - **不提供 strict-source 开关**（用户明确反对）：自动规划总是严格供给，缺原料的**正确做法是声明外部输入**（`external_inputs`），而不是放宽约束。
  - **异步**：规划在后台跑（py 实测 70s+），工具立刻返回 `project`/`factory` + `auto_plan.status=running` + `poll` 提示；agent 稍后 `get_planning_state {project, factory}` 读状态与结果。状态由**这次规划自己**写入 `AppState::auto_plans`，而不是让 agent 用 `recompute` 去猜——规划尚未回写时 `recompute` 算的是旧文档，会给出与计划无关的结果。
  - 配置阶段只跑**便宜的收敛命令**（品质上限 / 机器兼容 / 插件钳制），跳过 `Recompute`/`Persist`（最后一次规划统一落盘+重解），否则一次组合会触发六次整厂求解，「立即返回」就成了空话。
  - **实测验证逼出的两处修正**（都是「拿到成功、其实什么都没发生」这一类）：
    1. **一个调用曾经建出两个工厂**：工具先建工厂拿 id，`auto_plan_body_messages` 里又含一条 `add-factory`，于是项目里凭空多出一个 12 机制的空模板工厂（单测已加守卫）。
    2. **手写名字先校验、后动手**：`remove-enumerated-module` 对不存在的插件是**静默 no-op**（`retain` 找不到就报「无变化」），`add-to-external-input` 更把不存在的物品**原样写进文档**、规划照常报成功。现在 `item` / `modules.exclude` / `beacons[].beacon` / `beacons[].modules[].module` / `external_inputs[].flow` 一律在**建任何对象之前**按当前上下文的原型校验，错一个就整个调用失败并给候选（含错拼候选），一个对象都不建。
    3. 校验兜不住的部分（reducer 侧的失败，例如插件与机器不兼容）仍可能配置到一半：此时**回滚本次刚建的项目**，并在错误信息里说明是否回滚成功——绝不留下半个配置好的项目。
- **并发冲突语义**（原决策 45）；**端口/多实例/token 细节**（原决策 48）。

## 待办 / 下阶段

- [x] 真实跑一次应用，用 DSH 客户端连 `http://127.0.0.1:8765/mcp`，验证 `dispatch` 工具可驱动规划、GUI 实时刷新。
- [x] 收集 agent 使用 AppMessage 的感受 → 修正：`JsonSchema` 派生（inputSchema 真实化）+ `solve`/`scheduled_commands` 结构化 + 修正文档示例。
- [x] **补读取工具**（`get_planning_state`）——P0-1/P1-5 的核心，MVP 目前最大的盲区。
- [x] 采集使用反馈 → 抽离友好工具（原决策 6）：`get_planning_state` / `list_contexts` / `list_prototypes` / `localized_names` / `auto_plan` 已落地；`list_projects` / `list_factories` 被 `get_planning_state` 逐层读取覆盖后**已删除**；`suggest` 后来也**已删除**（易把 agent 引向手工加配方，且与 `auto_plan` 重叠）；`add_target` / `set_target_amount` / `add_mechanic` / `set_recipe` / `set_machine` / `load_context` 明确**不拆**（`dispatch` 即真实协议）。当前工具面 6 个。
- [x] 长时求解（`recompute`/`auto_plan`）走**异步**（类似 GUI 的 solving 事件），避免 MCP 调用期间占住 `Mutex<Runtime>` 导致 GUI 排队——原决策 47 的风险。见 `docs/runtime-concurrency.md`；`auto_plan` 另加「立刻返回 + `get_planning_state` 轮询状态」。
- [ ] 并发冲突语义（乐观锁/变更冲突提示）——原决策 45。
- [ ] 端口被占用 / 多实例 / token 传递细节——原决策 48。

## DSH 接入（验证用）

DSH 客户端配置见 DSH 仓库 `@deepseek-ai/dsh-mcp-client` 的 Streamable-HTTP 接法：命令（stdio）不用时，可改成 HTTP 端点 + `Authorization: Bearer <token>` 头部。本服务器**默认提供 HTTP 端点**，故 DSH 侧用 http transport 配置即可，工具名会带 `mcp__<serverName>__dispatch` 前缀。

## 手机接入（RikkaHub，局域网）

RikkaHub（Android）原生支持 MCP，传输类型选 **Streamable HTTP**（它还有 SSE，但我们的端点是 Streamable HTTP）。

1. 电脑上起服务（`<本机局域网 IP>` 用 `ipconfig` 里 WLAN/以太网那个，例如 `192.168.0.101`）：

   ```text
   metatorio-app --mcp-bind 192.168.0.101 --mcp-token <自己起一个长一点的随机串>
   ```

   启动日志会打印手机该用的完整 URL 与 Host 白名单；GUI 与 MCP 是同一个进程，手机上让 AI 改的文档会实时反映在电脑界面上（`document-changed`）。
   Windows 首次会弹「是否允许应用通过防火墙」，要**勾上专用网络**；没弹就手动加一条入站规则放行该端口。
2. 手机上：**设置 → MCP → + → Streamable HTTP**，填
   - **name**：随意，如 `metatorio`
   - **url**：`http://192.168.0.101:8765/mcp`（端口按 `--mcp-port`）
   - **headers**（自定义请求头，名称/值一对）：
     - 名称 `Authorization`，值 `Bearer <同一个 token>`（`Bearer` 后有**一个空格**）
3. 保存后应显示「已连接」并同步出 6 个工具；再到**助手的 MCP 服务器**里勾选这个服务器，工具才会进入对话。
4. 建议把会改文档的工具（`dispatch` / `auto_plan`）在 RikkaHub 里打开 **needsApproval**，让手机上每次真正动规划前都确认一次。
5. 排错顺序：手机浏览器先打开 `http://<IP>:<port>/mcp`（会看到 405/400 之类，说明网络通）→ 401 说明头没带对 → 403 说明 `Host` 不在白名单（换成 IP，或加 `--mcp-allow-host`）→ 连不上就是防火墙/不同网段（访客 Wi-Fi 常与主机隔离）。