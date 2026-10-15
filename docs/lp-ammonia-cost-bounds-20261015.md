# ammonia 量子点诊断与成本界实验（2026-10-15）

## 案例与环境

这是用户反馈的长期未解决案例，用户报告 microlp、HiGHS 和自研后端均遇到问题；不是新版本引入的性能回退。测试只读取原工程与应用上下文，复制到本地实验目录，不修改原工程或安装中的应用。

- 原工程：`D:/异星工厂/calc/ammonia量子点.json`。
- 可提交工程样例：`crates/metatorio-shell/src/mcp/auto_plan_assets_tests/fixtures/ammonia-quantum-dot.json`。
- 对应上下文：`735f5019abddbeba`，Factorio 2.1.19、ammonia 1.5.5、module-category-defaults 2.2.0。
- dump 本地副本：`local/ammonia-user-context-20261015.json`，SHA256 `56d0a53760393063e3fdec35c0ebf8e241e3f96f86dab5bff81fb87cac1a5a4b`。完整模组数据未加入源码仓库。
- 目标：普通 `quantum-dot-catalyst` 1/s，普通机器，无插件/插件塔；两个手动机制为配方与中子流燃料转换。中子流和配方物料已显式列为外部输入。
- 工程保存时是严格供给开启、严格输出关闭，星球 Nauvis、地表未指定。

## 诊断结果

原工程的两个机制直接重算，在严格输出关闭/开启两种情况下都得到完整精确最优解，成本均为 **5482.25**。原环境的全量自动规划在非严格输出下却生成不可行模型，并产生完整 Farkas 证明。两者不是同一个 LP。

直接配方 **`quantum-dot-catalyst`**（不是外部输入 `quantum-processor`）要求：

```json
"surface_conditions": [{ "property": "gravity", "min": 0, "max": 0 }]
```

当前 Nauvis 地表属性按该 dump 的默认值得到 gravity=10。手动工厂重算不做配方地表过滤，自动规划枚举会执行过滤，因此现成的直接路线未进入自动规划候选。原环境严格输出的自动规划进入较慢恢复过程后人工停止，记录为未完成，不计作无解。

对照实验仅把星球清空、地表设为 `space-platform`，不继承 Nauvis 免费矿源，不补充星岩或其他外部输入：

| 路径 | 严格输出 | 完整调用耗时（JSONL） | 成本 |
| --- | --- | ---: | ---: |
| 现有工厂重算 | 关 | 32 ms | 5482.25 |
| 全量自动规划及回写 | 关 | 175 ms | 5482.25 |
| 现有工厂重算 | 开 | 31 ms | 5482.25 |
| 全量自动规划及回写 | 开 | 203 ms | 5482.25 |

四项均有精确原始/对偶最优证据，全量候选模型为 3188 列、952 行，回写验证通过。这是在引入成本界优化前取得的结果，不能归功于新优化。它仅验证指定外部输入下的太空加工路线，不代表已实现完整星岩采集/物流模拟。

证据：`local/ammonia-baseline-existing-20261015.*`、`local/ammonia-baseline-full-20261015.*`、`local/ammonia-space-control-20261015.*`，原始 LP 与证书位于相应 `local/lp-exp/` 目录。

## 本轮修改

### 自动规划保留已有机制

根据用户后续请求，工具栏在“自动规划”旁增加“保留已有机制”，保存为项目偏好 `planning.preserve_existing_mechanics`，默认 false，旧文档通过 serde 默认值兼容。

- 关闭：原来的完整重新枚举、选优与替换行为。
- 开启：将已有的已启用手动机制与自动枚举候选去重合并；手动机制沿用手动工厂的环境/可达性规则，自动生成的其他候选仍照常过滤。
- 不强制旧机制有正流量，允许优化器选择更便宜的替代路线。
- 回写保留所有原条目的 ID、顺序和启用状态，只追加新的选中机制；未用旧条目可单独清理。
- 已禁用条目不作为手动候选，也不允许自动枚举同一配置后悄悄将它重新启用。

这使用户能明确选择“沿用手动指定路线并补全”，而不是通过取消所有自动枚举的环境限制来绕过问题。UI 提示明确说明手动机制保留环境例外。

### 成本界与诊断

1. 自动规划失败时，若当前手动工厂的已启用配方不满足环境条件，错误信息附上配方名称、条件，以及手动重算和自动规划过滤的区别。仅补充诊断，不绕过环境条件，也不把某条手动路线不合法当作所有替代路线不可行的证明。
2. 增加真实工程的独立测试入口。指定 `AMMONIA_TEST_SURFACE=space-platform` 时清空星球以避免继承地面免费资源；不设置时验证原工程重算与非严格自动规划的差异。
3. 增加精确成本界分析：只有完整原模型可行的 incumbent 才提供成本预算。预算剔除目标常数项；正成本变量得到有限上界，零成本变量保持未知上界，随后用物料行和成本剩余量传播上下界。重复系数先聚合，最多 8 轮并遵循取消控制。
4. 增加成本界浮点提示实验，最多附加 64 条向外舍入的上界行。只在额外松弛变量均在基内时移除这些行，还原原模型的方阵基；否则放弃该提示。任何最终结果仍检查完整原模型。受限搜索失败不产生原模型无解结论。

成本界路径**默认关闭**。显式设置 `METATORIO_LP_COST_BOUNDS=1` 后，仅在原有三轮提示失败、且已取得精确可行 incumbent 时尝试，最多共享总预算中的 15 秒。保留实验开关是为了先做固定输入对照，不因单元测试通过就宣称实际加速。

## 复现入口

```powershell
$env:AMMONIA_CONTEXT_DUMP = 'D:\rust-projects\metatorio\local\ammonia-user-context-20261015.json'
$env:AMMONIA_TEST_REPORT = 'D:\rust-projects\metatorio\local\ammonia-new-run.jsonl'
$env:AMMONIA_TEST_SURFACE = 'space-platform'
cargo test --release -p metatorio-shell --lib ammonia_quantum_dot_existing_and_auto_plan -- --ignored --nocapture
```

报告路径要求新文件，避免覆盖历史证据。原环境对照时移除 `AMMONIA_TEST_SURFACE`。

在原工程 Nauvis 环境验证保留模式时，同时移除 `AMMONIA_TEST_SURFACE` 并设置 `AMMONIA_PRESERVE_EXISTING=1`。

成本界对照工具：

```text
cargo run --release -p metatorio-solver --example lp_cost_bounds -- <原模型快照> <精确见证> <新报告路径>
```

工具默认使用给定精确见证作为 incumbent，比较无界提示与成本界提示。可选第四个参数指定流量缩放比例，但缩放后必须重新验证可行性；本例的目标是固定产量等式，不能直接把流量放大两倍。这也是不能把生产模型一概视为可同比缩放的实际例子。

## 最终验证与实验取舍

- 原环境保留模式：严格输出关闭/开启的完整自动规划分别 **217 / 183 ms**，成本均 **5482.25**，全量 LP 3210 列、964 行。精确最优与回写验证通过。
- 原环境替换模式：现有机制重算成功，全量自动规划仍在约 1593 ms 取得原生成模型的无解证明，并给出含 `quantum-dot-catalyst` 地表条件的提示；测试按预期通过。
- 纯太空对照：全量自动规划严格输出关闭/开启 **186 / 190 ms**，成本相同，不继承地面矿源。
- 当前三组共 **19 个原模型**由独立验证器重新检查：18 个精确最优、1 个完整 Farkas 无解证明，舍入检查通过。原环境严格输出自动规划的早先中止记录未混入这些证书。
- 核心 **63 项**、求解器 **38 项**、Runtime **79 项（另 2 项忽略）**测试通过。核心/求解器全目标 Clippy 与 Runtime/Shell 生产库 Clippy 通过。覆盖整个 Runtime 测试目标的 Clippy 还指出 3 项原有测试样式问题；未作为本轮 LP 功能修改处理。
- 前端 `npm run check`：0 errors，10 warnings（2 个文件）；未启动 GUI 做视觉操作验证。

成本界固定输入对照使用纯太空非严格全量 LP 的精确最优见证：3188 个变量均得到有限上界，未推导出正下界；分析约 **5063 ms**。原提示 **11 ms / 29 pivots**，成本界提示含其分析开销 **4902 ms / 29 pivots**，两者均通过完整原模型精确最优认证。两次分析分别发生于统计和实际提示，不能把两项耗时误认为同一次调用的两个阶段。此案例没有收益，因此实验保持默认关闭，不声称性能改善。

证据更新：`local/ammonia-current-{original,preserve,space}-20261015.{log,jsonl}`、对应 `*-verify-20261015.log`、`local/ammonia-cost-bounds-comparison-20261015.json`、`local/lp-preserve-runtime-final-tests-20261015.log`、各 Clippy 与前端检查日志。
