## 功能缺陷

1. ~~自动规划会枚举当前表面不能建造的机器~~ **已修复**：机器候选在挑选阶段就按 `surface_conditions` 过滤（含发电/锅炉/反应堆/太阳能与插件塔实体），且「无解锁机器」的退化分支不再把不可建造的机器放回来。
2. ~~不能吃插件塔效果的机器运行添加插件塔~~ **已修复**：`EffectReceiver.uses_beacon_effects` / `uses_module_effects` 现同时约束自动规划枚举、求解计算（效果与耗电）与前端插件塔编辑入口。

## MCP / AI 体工学（按优先级；结论来自对 live 端点 http://localhost:8765/mcp 的实测）

- ~~`dispatch` 创建对象后直接返回新 id~~ **已修复**：`DispatchResult` 新增 `created`（projects / factories / mechanics / targets / target_expressions / target_terms / external_inputs，按创建顺序），MCP `dispatch` 原样回传；新建工厂模板自带机制、克隆机制、自动规划回写的机制 id 也一并报告。agent 不再需要「创建后再读一遍」。
- ~~写入前校验原型名~~ **已修复**：新增 `metatorio-runtime/src/validate.rs`，`Runtime::dispatch` 在进 reducer 前用项目当前上下文校验消息引用的原型名（配方 / 机器 / 资源 / 物品 / 流体 / 科技 / 星球 / 地表 / 品质，含插件与插件塔、燃料、枚举偏好、建议候选）；不存在的名字返回 `InvalidValue`，不再静默写入垃圾。拿不到 store（项目未绑定/未载入上下文）时跳过，不阻塞。
- ~~求解失败要让 agent 看见~~ **已修复**：`execute_command` 改为返回 `CommandOutcome { effect, errors }`，MCP `dispatch` 回传 `errors: [...]` 并在有失败时置 `is_error = true`（求解 / 自动规划 / 清理 / 落盘 / 打开工程 / 关闭项目 / 上下文载入的失败，以及未实现命令，都不再静默）。
- ~~求解结果的可读量~~ **已修复**：`MechanicSolution` 新增 `rate = amount / scale`（可比量）与 `is_virtual`（展开阶段转换流辅助变量，`mechanic` 为 u64::MAX、不对应文档机制）；`FlowBalance` 同样补 `rate`。
- ~~版本冲突检查收窄到工厂~~ **已修复**：`Runtime::document_matches` 不再比较整份文档的全局 `revision`，改为比较**目标工厂文档**（机制 / 目标 / 外部输入 / 工厂设置）+ 项目设置与规划偏好 + 上下文实例与可达性代次；别的工厂改名不再误拒自动规划回写。
- ~~读取粒度~~ **已修复**：新增 `list_projects` / `list_factories` 两个轻量索引工具（项目：id/名称/上下文/工厂·机制·目标计数；工厂：id/名称/星球·地表/主品质/严格供给/计数 + 目标清单），agent 先看索引再决定读哪个完整文档；`get_planning_state` 的 `recompute` 只在 project + factory 同时给出时有效，其余组合显式报错（不再静默忽略）。（字段/机制裁剪按设计稿的正交性原则不做——那是 agent 侧 JSON 工具的事。）
- 单位标注：内部量纲是「每秒」（实测把项目 `time_scale` 改成 minutes 不改变任何求解数值，只影响显示），但工具 schema 未说明，agent 容易按「每分钟」填目标。
- ~~幂等/重试~~ **已修复**：MCP `dispatch` 新增可选 `request_id` 幂等键——同一 id 只应用一次，重试直接回放上次载荷（带 `idempotent_replay: true`）。进程内 `DispatchCache`（有界 FIFO，256 条，成功与失败都记）。
- ~~自动规划结果与当前一致时跳过回写~~ **已修复**：`auto_plan::same_mechanics` 做与顺序无关的等价判定（忽略条目 id / enabled），等价时不再回写——省掉 revision bump、落盘与求解缓存失效；随后直接重解一次（命中缓存）以回传结果。
- ~~长时间计算任务的边界处理~~ **部分修复**：`SolveJobs` 增加等待上限（默认 120s，`METATORIO_SOLVE_TIMEOUT_MS` 可调）——超时返回可重试的结构化错误，后台任务继续跑完（句柄 drop 不取消），调用方不会被失控求解钉住；失败不再写入幂等缓存，所以能用同一个 `request_id` 重试。**待续**：真正的「发起即返回 + 完成后再回报」需要 job 模型（工具返回 job_id + 查询工具），属于接口形态决策。
- ~~无头模式：不启用 GUI 跑 MCP~~ **已实现**（路径 A）：启动参数用 clap 解析（`--headless` / `--mcp-port` / `--mcp-token` / `--solve-timeout-ms` / `--no-mcp`，各自都有同名环境变量回退，优先级 CLI > env > 默认；`--headless` 与 `--no-mcp` 同时给出会直接报错退出，因为那等于「没有窗口也没有接口」）。无头的实现方式：在 `Builder::build` 之前清空 `tauri.conf.json` 的窗口配置（窗口是事件循环首次迭代的 `setup` 阶段才创建的），`setup` 照常跑（注册表扫描、恢复最近上下文、启动 MCP），因此**一个窗口都不建**，事件循环也不会因「最后一个窗口关闭」而退出。MCP 服务器改为在 `setup` 开头启动，所以端点在上下文恢复（可能数十 MB dump）之前就已可用。实测（debug 版）：headless 进程的真实应用窗口数为 0（只剩 tao 的两个 `WS_EX_TOOLWINDOW` 辅助窗口，0×0 / 13×13），端点约 0.7s 可用、`--no-mcp` 的 GUI 仍有 1 个窗口且不开端口。**Linux 仍会初始化 GTK/X11（需 xvfb）**——这是 tauri 的运行时行为，未改动；要彻底去掉需路径 B（独立 `metatorio-mcp` 二进制，与 mcp-design 决策 1「同一进程共享 AppState」相悖）。
- ~~无头版本的游戏没有贴图信息，ai尝试自行导出上下文时会在导出贴图时失败~~ **已修复**：`--dump-icon-sprites` 改为 best-effort（失败只告警，上下文照常注册、只是没有图标）；只有真的导出出贴图目录时才把它当图标源，避免把 `script-output`（含 dump）整个搬进图标目录。
- ~~导出 config 只写 `write-data` 不写 `read-data`~~ **已修复**：config.ini 增加 `read-data=<游戏安装目录>`（从可执行文件路径推断 `bin/x64`、`bin`、根目录三种布局，并确认存在 `data/` 才写；找不到则省略交给游戏自己找）。