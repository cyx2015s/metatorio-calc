# Runtime 并发与锁纪律

> 背景：`AppState` 只有一把 `Mutex<Runtime>`，而 `Runtime` 里混着四类性质完全
> 不同的东西——可变文档、大块只读原型仓库、派生缓存、以及需要数十秒的求解。
> 早期实现里 `dispatch` **一上来就上锁**，并在持锁状态下执行完所有副作用
> （`execute_command`），于是任何一次编辑触发的求解/落盘都会阻塞 MCP 工具
> 调用、GUI 的其它交互、甚至只读快照，也让不同工厂无法并行规划。

本文记录现在的锁纪律，改代码前请先读这一页。

## 三条规则

1. **R1 — 锁内只做「读快照 / 写增量」**，不做纯函数计算。
2. **R2 — 任何 >1ms 的工作（LP、BFS、JSON、fs、网络、子进程）一律移出锁**，
   用版本戳回写。
3. **R3 — 派生结果各自用自己的缓存锁**，缓存键必须包含输入版本。

## 结构

```
AppState {
    runtime: Mutex<Runtime>,          // 只保护「状态变更 + 短读」
    solve_jobs: SolveJobs<SolveResult>,          // 求解调度（单飞 + latest-wins）
    autoplan_jobs: SolveJobs<(SolveSnapshot, Vec<Mechanic>)>,
    context_loads: KeyLocks,          // 同一上下文只读盘解析一次
    contexts: Mutex<ContextRegistry>, // 上下文清单（磁盘元数据）
    project_paths / locales: Mutex<...>,
}

Runtime {
    state: RuntimeState,                        // 文档 + revision + dirty
    contexts: HashMap<String, Arc<PrototypeStore>>,  // Arc：取快照是 O(1)
    accessibilities: Mutex<HashMap<ProjectId, Accessibility>>,
    accessibility_epoch: AtomicU64,             // 每次失效 +1
    graph_cache: Mutex<HashMap<String, Arc<GraphData>>>,
}
```

`dispatch` 只把 **reducer** 放进短临界区，随后逐条执行命令，每条命令自己
决定锁的边界：

```rust
let outcome = { state.runtime.lock()?.dispatch(message)? };   // 微秒级
for command in &outcome.commands {
    execute_command(&app, &state, command).await;             // 各自短锁 / 锁外
}
```

## 快照—计算—发布

求解只依赖一份快照，因此可以脱离锁在任意线程上跑：

- `Runtime::solve_snapshot_inputs(project, factory) -> SolveSnapshot`
  （锁内，微秒级）：Arc 拷贝 store/依赖图、克隆项目/工厂文档、带上
  `revision` 与 `accessibility_epoch`。
- `solve_snapshot_with(&snapshot, &accessibility)` / `plan_auto_plan(...)`：
  纯函数，锁外执行。
- `SolveSnapshot::resolve_accessibility()`：冷缓存时现算可达性（py 上下文
  约 2.5s），同样在锁外。
- 回写一律带版本校验：
  - `cache_accessibility_if_current(project, revision, epoch, a)` —— 只有文档
    版本与失效代次都没变才写入，否则会留下过期可达性。
  - `document_matches(&snapshot)` —— 自动规划/清理回写前校验，避免覆盖用户
    在规划期间的编辑。

项目级只读重计算用 `ProjectSnapshot` + `productivity_view` / `ordered_milestones`。

## 求解调度（`solve_jobs.rs`）

- **同 `(project, factory)` 串行、不同工厂并行**（每键一把异步锁，计算跑在
  `spawn_blocking` 上）。
- **latest-wins 合并**：快照在拿到键锁**之后**才取，排队请求因此看到最新文档；
  若身份（文档版本 + 可达性代次 + store 实例）与上次一致，直接复用结果。
- **求解期间文档又变了 → 再算一轮**（最多 `MAX_ROUNDS` 轮）。
- 失败不缓存。

## 命令分类

| 类别 | 命令 / 路径 | 处理方式 |
| --- | --- | --- |
| 状态变更（微秒） | reducer、`EnsureQualityLimit`、`ClampModules`、`EnsureMachineCompat` | 短锁 |
| 派生计算（长，可丢弃） | `Recompute`、`AutoPlan`、`Cleanup` | 锁外算 → 版本校验 → 回写 |
| 只读重计算 | `accessibility` / `productivity` / `milestones_ordered` / `mechanic_flow` / `solar_balance` / `implicit_sources` / `catalog_index` / `suggest` / `allowed_modules` | 锁内取快照 → 锁外算 → 短锁回填缓存 |
| 需 store 的项目级批量写 | `SetDefaultMilestones`、`UseBestModules`（`planning.use-best-modules`） | 在 `Runtime::dispatch` 进入 reducer 前解析（reducer 无 store），再走 `finish`：revision/dirty/Persist/Recompute |
| 外部 IO（长） | `Persist`、`LoadProject`、`LoadGameContext`、`LoadCachedContext`、`CloseProject` | 锁内取/写增量，读盘/写盘/子进程在锁外 |

`Outcome` 的构造器命名即意图：`meta`（只落盘，不求解）/ `solve_factory` /
`solve_all`。改名、排序等纯元数据变更走 `meta`，不再白跑一次整厂求解。

`execute_command` 的 match **没有通配分支**：新增 `RuntimeCommand` 变体会在
编译期暴露，而不是静默不执行。

## 已知剩余项

- `icon` 仍是同步命令，在主线程上读 PNG 文件（前端已缓存图标，影响有限）。
- 「默认里程碑」（`ProjectAction::SetDefaultMilestones`）在锁内遍历全部实体推导
  科技瓶集合（一次性操作）；它已收敛进 `dispatch` 管线，改文档后由 `finish`
  统一递增 revision、落盘并重解全部工厂。
- LP 求解器本身不可中断：现在的策略是「不启动过期任务」，不做 mid-solve 取消。
- 两个客户端同时改同一工厂仍是「后写覆盖」；求解结果按 revision 丢弃过期值，
  但文档层面的冲突提示尚未实现（见 `docs/mcp-design.md` 决策 45）。
