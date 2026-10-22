# 自动规划期间显示 LP 规模

2026-10-22：LP 实验提交 `013168f` 已快进合并至本地主分支 `master`。本功能在主分支上追加。

机制列表工具栏在自动规划期间显示阶段、变量数和约束数：

- 准备／枚举／建模期间，数量尚未知，显示“变量数、约束数待构建”。
- LP 建好后显示完整问题规模，包含外部输入／输出等辅助变量；约束数为显式约束表达式数，不另外统计变量上下界。
- 求解期间持续保留这些数字，不因普通重算事件而消失。它们不是后端内部缩减后的工作集规模，也不是完成百分比。
- 回写前验证会显示“验证规划”，并更新为验证子模型的规模。
- 完成、失败、命令等待超时或命令取消时，结束该次规划显示。超时后仍运行的阻塞工作线程不能重新激活显示。

`SolverData::solve_with_progress` 在实际构建完 LP 后报告规模；runtime 提供 `plan_auto_plan_with_progress`，原有无观察者入口保持兼容。shell 通过现有 Host 事件机制广播 `auto-plan-progress`，GUI 和 MCP 共用该命令执行路径。事件包含 project、factory、单调递增 run_id、running、stage、variables、constraints。前端按工厂保存进度，忽略已经结束或被新请求取代的旧事件。

验证入口：

```powershell
$env:CARGO_INCREMENTAL='0'
cargo test -p metatorio-solver -p metatorio-runtime --lib -- --test-threads=1
cargo test -p metatorio-runtime --lib auto_plan_progress_covers_full_model_and_writeback_verification
cargo test -p metatorio-shell --lib auto_plan_progress::tests
cargo clippy -p metatorio-solver -p metatorio-runtime -p metatorio-shell --all-targets -- -D warnings
```

前端：`pnpm check`、`pnpm build`、`pnpm test:auto-plan-progress-browser`。浏览器测试沿用 `PLAYWRIGHT_MODULE` 和 `BROWSER_EXECUTABLE`，可用 `AUTO_PLAN_PROGRESS_SCREENSHOT` 保存截图。该测试在真实 Svelte 页面上模拟 IPC，检查数字持续显示、验证阶段更新、工厂隔离、MCP 来源、旧消息以及成功／失败清理；不声称完成真实游戏大模型的性能测试。

本轮前端检查、生产构建、浏览器测试通过；solver 常规 46 项及 runtime 常规 85 项通过（另有 2 项忽略），包含新增的完整规划阶段／规模测试；shell 结束清理与迟到事件 2 项测试通过。浏览器截图位于 `local/auto-plan-progress-20261022.png`。

Clippy 普通模式可完成；`-D warnings` 被原有代码告警阻断，包含旧 `CommandEffect` 大枚举、旧测试的 clone／len／默认值赋值及 shell 原有条件／结构更新，未以放宽全局 lint 或修改无关代码来处理。日志保存在 `local/auto-plan-progress-clippy-20261022.txt`。
