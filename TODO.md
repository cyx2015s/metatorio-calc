## 功能缺陷

1. ~~自动规划会枚举当前表面不能建造的机器~~ **已修复**：机器候选在挑选阶段就按 `surface_conditions` 过滤（含发电/锅炉/反应堆/太阳能与插件塔实体），且「无解锁机器」的退化分支不再把不可建造的机器放回来。
2. ~~不能吃插件塔效果的机器运行添加插件塔~~ **已修复**：`EffectReceiver.uses_beacon_effects` / `uses_module_effects` 现同时约束自动规划枚举、求解计算（效果与耗电）与前端插件塔编辑入口。

## MCP / AI 体工学（按优先级；结论来自对 live 端点 http://localhost:8765/mcp 的实测）

- `dispatch` 创建对象后直接返回新 id：现在只有 project/factory 能从 `scheduled_commands`（persist / ensure-quality-limit / recompute）间接读出，mechanic 与 target 完全读不到，agent 每个创建动作都要再补一次 `get_planning_state` 往返。
- 写入前校验原型名：`set-recipe` / `set-machine` 传不存在的名字（实测 `not-a-real-recipe`、`not-a-real-machine`）返回 `changed: true` 并把垃圾写进文档——reducer 拿不到原型仓库，无法校验。应报错，或至少返回 warning。
- ~~求解失败要让 agent 看见~~ **已修复**：`execute_command` 改为返回 `CommandOutcome { effect, errors }`，MCP `dispatch` 回传 `errors: [...]` 并在有失败时置 `is_error = true`（求解 / 自动规划 / 清理 / 落盘 / 打开工程 / 关闭项目 / 上下文载入的失败，以及未实现命令，都不再静默）。
- 求解结果的可读量：`solve.mechanics[].amount` 是 Ruiz 缩放空间的原始值，可比量是 `amount / scale`（代码注释里有，schema 里没有）；另外 `mechanic: 18446744073709551615` 是展开阶段引入的「转换流」虚拟变量，不是文档里的机制 id，容易被当成悬空引用。
- 版本冲突检查收窄到工厂：`revision` 是整份文档的全局版本，`Runtime::document_matches` 用它做自动规划回写前的校验，于是「A 工厂规划期间人在 B 工厂改了个名字」也会让回写被拒（报「文档在自动规划期间被修改」）。需要工厂级版本或内容哈希，否则会误伤并行规划——而那正是这次锁改造的目标。
- 读取粒度：`get_planning_state` 取单个工厂也要返回整份工厂文档（大项目几十个机制），且 `recompute` 在 project 层被静默忽略。补 `list_projects` / `list_factories` 与字段/机制过滤，非法参数组合显式报错。
- 单位标注：内部量纲是「每秒」（实测把项目 `time_scale` 改成 minutes 不改变任何求解数值，只影响显示），但工具 schema 未说明，agent 容易按「每分钟」填目标。
- 幂等/重试：`dispatch` 超时后重试会重复添加 target/mechanic；没有 request id 或幂等键。
- 自动规划结果与当前一致时跳过回写：回写会 bump revision，使 `SolveJobs` 的身份缓存必然失效（实测连续两次 auto-plan 都重算）；结果没变就不该改文档。
- 长时间计算任务的边界处理（群友反馈：还有边界，有没有写硬返回，用不能硬算10分钟算到死吧）（怎么实现发起计算请求后立刻返回，在计算完成时再发回结果？）
- 求解需要的游戏上下文在无头环境怎么导出（群里的AI自己配置了xvfb，能跑了，以后需要支持完全的无头模式，不启用gui，也要能开关mcp模式）
- 无头版本的游戏没有贴图信息，ai尝试自行导出上下文时会在导出贴图时失败
- 导出 config 只写 `write-data` 不写 `read-data`，游戏会去 `/usr/share/factorio` 找数据包——我在沙盒里软链绕过，正式环境建议补上 `read-data` 指向游戏安装目录