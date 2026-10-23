# 可行解提前返回与最优性报告

默认 `rust-certified` 保留完整相关列上的浮点搜索及原模型认证快路径；搜索失败时，小路线作为可行性备选。找到完整原模型的精确可行基后，可以继续列生成和原始目标优化。可行性与最优性分别报告。搜索时限结束时返回最近已认证的可行点，不再强求最优性或对偶差距证书。显式用户取消仍返回取消错误，不触发写回；尚未找到可行点的超时也不构成不可行证明。

默认单次 LP 搜索预算为 300 秒（初版为 60 秒，后续 Fulgora 品质插件 3 回归证实其留给优化的余量不足），可通过 `METATORIO_LP_SEARCH_SECONDS` 配置正有限秒数。输入解析、建模、最终精确认证、浮点输出检查和自动规划写回检查可能使总耗时超过搜索预算。最终认证不能为赶时间而跳过。保留原有 relative balance/gap 均为 `1e-12` 的候选验证快路径（`METATORIO_LP_BALANCE_MODE=exact` 可关闭）；新增的超时返回路径仍要求精确原模型可行及浮点输出检查通过。`METATORIO_LP_ROUTE=1` 可显式试验路线优先，`0` 可关闭路线备选；默认不抢占完整相关列浮点搜索。

`OptimizationReport` 随求解结果保存（副产物复现及后续改进见 [Fulgora 记录](lp-fulgora-quality3-20261023.md)）：

- `optimality_proven` 仅来自完整原模型精确 primal/dual 最优证书。
- `termination_reason` 区分 `optimal`、`time-limit`、`search-stopped`。
- `recent_pivot_cost_changes` 保存最近 8 次已完成且可表示为有限 f64 的原目标 primal 换基：序号、前后成本与下降量。下降量先用有理数相减，再转 f64；保留退化换基的零下降。不计 Phase I、修复目标或切换 restricted master 造成的虚假差值。

内核每次换基保存稀疏的基本结构变量和受约束行，不复制百万列的全零解向量；段尾或超时恢复时才提升到完整原模型并认证。非超时的数值错误不转换成超时成功。

自动规划的完整候选报告保存在 `SolveDiagnostics.planning_optimization`；回写后工厂子模型的报告保存在 `optimization`。界面优先展示前者，防止将子模型最优误称为全局规划最优。旧结果没有这项报告时不推断新结论。

## 验证

内核回归覆盖初始及换基中途超时、取消、pivot limit、最近 8 条窗口、跨 master 段、零下降与浮点前后成本舍入相同。应用适配回归检查提前返回的原模型可行点和状态。runtime/shell 回归检查 JSON 兼容与完整候选报告跨写回保存；Svelte SSR 测试检查实际显示内容。

### 普通品质 lengden 回归

使用相同输入项目、上下文，目标仅改为普通品质轻质框架 120/s。实际冻结完整 LP 为 1,188,032 列、3,489 行。

末尾核对下载目录中的最新 `lengden.json`：与冻结输入的唯一 JSON 内容差异正是目标品质 `legendary → normal`，因此上述普通品质实验输入与最新文档内容一致。

- 路线优先：完整自动规划 78.452 秒；LP 搜索 60.732 秒超时，成本 1290.2473107422222。记录 1499 次精确 primal 换基，最后 8 次下降均为 0；回写校验通过，但完整候选规划未证最优。
- 同一冻结 LP，禁用前置路线、恢复原默认 balance/gap 验证：浮点搜索 1023 次换基耗时 8.549 秒；适配器搜索/认证 13.81 秒，含外围适配 14.907 秒。完整原模型精确 primal/dual 最优证书成立，成本 930.8037891754781。pricing scale 重算在后期每次约 6–8 毫秒，未支持其为本次主因。
- 修复默认行为为浮点优先、路线失败备选；不按品质或物品名特判。CI v1.5.45 与本地默认后端均为 Rust，并非 HiGHS 切换导致。

实验产物位于 `local/lengden-cold-route-20261023/normal-anytime/`、`normal-float-priority.json` 及同名前缀日志。最终默认入口整项目复验 `normal-default-fixed/`：总计 **33.618 秒**（进程 wall 34.422 秒），完整候选 LP 搜索/认证 **13.637 秒**，成本 **930.8037891754781**。完整候选和回写子模型均有精确最优证书，写回验证通过，38 个实际机制。与路线优先相比耗时减少约 57.1%，成本减少约 27.9%。同一上下文、输入文件及构建的 SHA256 保存在各目录 `harness.json`。

### 原传说目标默认入口

`legendary-default-fixed/` 使用同一构建及上下文，原传说目标 120/s，不提供历史种子、默认浮点优先与 60 秒搜索预算。完整自动规划 **81.101 秒**（进程 wall 81.818 秒）成功退出并写回 **20 个机制**，写回校验通过。完整候选优化报告 `time-limit`、`optimality_proven=false`，耗时 61.876 秒；最后 8 次换基为第 412–419 次，下降量均为 0，原规划成本约 120701.26669。回写子模型另有最优证书，不能将该证书替代完整候选报告。

搜索时限不是整个 API 调用的硬墙时限：本实验还包含候选枚举、完整 LP 构建、捕获文件、可行点最后认证与写回校验。超时提前返回的方案成本仍较高，本轮解决的是可用返回与正确报告，不声称已找到高质量传说路线。

### 短预算与回归

传说品质完整冻结 LP 的路线优先 5 秒实验：适配器总计 6.745 秒返回可行点，精确 primal=true、optimal=false，成本 132409.97873893764，28 个非零变量；尚未发生可记录的原目标优化换基，记录为空。该结果证明超时可行点不会被丢掉，不证明质量较高。

五个小型可行 fixture 均精确证最优；有损闭环 fixture 保持结构化不可行结果。前端 `npm run check` 零错误/警告，`npm run test:optimization` 6/6。LP 核心 145 通过、2 忽略；solver 61 通过、4 忽略；runtime 88 通过、2 忽略（Windows 默认临时目录的文件改名测试遇到拒绝访问，改用工作区 TEMP/TMP 后完整通过）。shell 报告跨写回回归通过。

## 内点法的边界

内点法可能减少困难优化阶段所需迭代，但需要比较完整耗时：矩阵处理、Newton 系统解算、填充/条件数、crossover，以及最后的有理数认证。本轮未切换默认后端，也未把理论可能性当作 lengden 的性能结论。

内点解中的小正数不是精确零的证明。适合当前写回语义的流程是：内点候选 → crossover 得到基 → 原始系数的有理数重构 → 原模型认证。由该基重构后，非基本且位于零下界的变量才是返回方案中精确为零的变量；基本退化变量也应根据精确值判断。返回方案为零不等于所有最优方案都为零。若另有精确最优对偶证书，严格正的约化成本结合互补条件可以证明对应零下界变量在每个最优解中为零；零约化成本不能作反向推断。

参考：[HiGHS solver documentation](https://ergo-code.github.io/HiGHS/dev/solvers/)、[Gurobi crossover explanation](https://support.gurobi.com/hc/en-us/articles/360013196832-Does-the-barrier-algorithm-return-a-basic-solution-for-LPs)、[Barrier factorization and crossover logging](https://docs.gurobi.com/projects/optimizer/en/current/concepts/logging/barrier.html)、[MIT complementary slackness notes](https://courses.csail.mit.edu/6.854/21/Scribe/s12-duality/s12-slackness.html)。
