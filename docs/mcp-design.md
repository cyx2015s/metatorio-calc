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

要我把上面整理成一份**可写进仓库的设计稿**（比如 `docs/mcp-design.md`，含"决策理由"树，方便下轮 compact 接力），还是就以这段对话为准等 compact 自动节选？另外：是否需要我下轮从"工具面枚举 + 异步求解"继续（Phase 2 规划细化），还是先停在架构决策这层？