# 代码精简与性能审查（2026-10-28）

本轮已落地可验证的去重与性能改动，**尚未达到整个代码库 SLoC 减少 20% 的目标**。统计不包含第三方 vendor、游戏 JSON 数据、构建输出和 Markdown，不以删除测试或压缩代码排版计算收益。

## 统计口径

2026-11-01 按用户澄清，后续以**手写生产代码 SLoC**作为精简 20% 的主指标。测试（包含 Rust 源文件内的 `#[cfg(test)]` 模块、测试辅助代码和浏览器回归夹具）、自动生成代码、示例和开发脚本分别列出，不计入生产代码。新增回归测试不抵消生产代码的精简收益。第三方依赖、数据及构建产物继续排除。

原始提交与当前提交需要使用同一分类规则重新统计，才能报告该主指标的减少比例；不能直接将旧的全库基线与排除测试后的当前值比较。仅通过目录排除会漏掉 Rust 内联测试，正式分项统计必须同时处理这些代码。以下数字保留为原口径的历史记录，全库总量作为辅助指标继续追踪，不代表已按新口径完成重算。

使用原始 HEAD 和最终文件分别执行：

```powershell
tokei crates metatorio-app scripts --types Rust,TypeScript,JavaScript,Svelte,CSS,HTML,Python,PowerShell -o json
```

该口径包含生产代码、测试、示例及脚本。原始基线为 **88,766 SLoC**，最终为 **88,140 SLoC**，净减少 **626 行（0.705%）**。20% 对应至少净减少 17,754 SLoC，目前还差 17,128 行，不能报告为目标已完成。

## 已落地改动

| 路径 | 改动及其作用 |
|---|---|
| `metatorio-core/src/expand.rs` | 16 个具体机制展开函数返回 `Option<Flow>`，从类型上表达至多一个变量；统一附加成本和供暖系数，移除泛型输出列表及分支编号的准备工作。 |
| `metatorio-core/src/temp_flow.rs` | 删除未被生产机制调用的 `add_parallel`、流副本列表及中间变量列表；直接构造单份稀疏流。 |
| `metatorio-core/src/prim_var.rs` | 删除只有 `inner` 字段的 `PrimVar<C>` 包装，展开结果直接携带 `config`，配置 ID 无需 `Clone`。 |
| `metatorio-runtime/src/solve.rs` | 删除 `variant`、两个逐机制分支计数表及结果字段；实际存在的辅助转换流使用独立 `Conversion(usize)` ID，避免覆盖，也不再受 u16 编号范围限制。 |
| `metatorio-runtime/src/solve.rs` | 宇宙候选直接移入数组并按连续 ID 查回，删除整份候选机制的 HashMap 副本；表达式直接克隆已有类型。 |
| `metatorio-runtime/src/auto_plan.rs` | 单遍收集非零真实候选，删除未被生产路径使用的重复 `support` 列表和中间用量数组；诊断 cutoff 保留，不参与删选。 |
| `metatorio-runtime/src/state.rs` | 合并机制类型与动作的匹配、历史重算分支；共用对象 ID 遍历，删除未读取的工厂 ID 映射。 |
| `metatorio-runtime/src/state.rs` | 导入时一次建立已有 ID 集合，每追加项目后更新，替代每个嵌套对象都扫描整个已有文档；保留原有重编号顺序与历史行为。 |
| `metatorio-runtime/src/universe.rs` | 合并字段和处理行为完全相同的 Local/Transport 来源记录。地点仍属于流键和结果，不隐式共享物料、能量或运输容量。 |
| `metatorio-shell/src/app.rs` | 两处整份 `PrototypeStore` 深拷贝改为现有 Arc 接口；隐式流排序缓存 Debug 键，避免每次比较重新分配字符串。 |
| `metatorio-shell/src/app.rs`、`mcp.rs` | 目录 DTO 共用默认值，删除重复初始化字段。部分完全等价的手写 Default 改为派生。 |
| 前端类型、浏览器夹具 | 同步移除求解结果的 `variant`。其它同名概念，如图标原型 variants、CSS 数字样式和 LP 候选品质组合，不属于本次删除范围。 |

Rust API 的明确变动：`ExpandedVariable.prim_var.inner` 改为 `config`；`ExpandedVarId` 区分真实机制与转换流；`MechanicSolution.variant`、`UsedCandidates.support` 及预留并行展开接口被删除。仓库调用点已同步。旧求解结果 JSON 中多余的 `variant` 字段仍可由 serde 忽略。

## 性能对照

独立 release 微基准构造 10,000 个嵌套目标 ID 冲突，运行真实 `RuntimeState::import_projects` 七次，计时不包含初始状态构造：

| 原始 HEAD 中位数 | 修改后中位数 | 比值 |
|---:|---:|---:|
| 18.689 ms | 2.276 ms | 约 8.2 倍 |

两次基准导出的完整文档 SHA-256 均为 `65B73952D086B0EDDC41A7DE9E413A7E3291D6CD48E833E1B23937FDDE4AC28E`。这只证明该导入场景的性能与输出一致性，不代表 LP 求解或整个应用同样加速。

源代码还明确消除了两次原型仓库深拷贝、候选副本和每机制的中间列表分配；未把这些静态收益换算为未经测量的整机加速百分比。基准程序、JSON 及日志保存在 `local/import-bench/` 和 `local/import-*-benchmark.log`。

## 回归边界

原始 HEAD 使用相同 Ninja/VS 环境执行 `cargo test --workspace --all-targets --no-fail-fast`：772 通过、1 失败、25 个既有忽略项。失败为 `epic_small_circuit_with_shared_beacons_preserves_solvability` 的 `1e-6/s` 目标，回写重解缺少上游供给。

按用户明确授权，改为 `1e6/s` 并更名为 `epic_large_circuit_with_shared_beacons_preserves_solvability`，品质、插件塔配置及所有可行性、供给、有限数值和持久化文档重解断言保持。原有 `1/s` 史诗电路用例也保留。这个调整不表示极小目标问题已经修复。

新增覆盖包括：所有嵌套对象 ID 冲突、后续导入项目的 ID 预留、导入撤销重做、非 Clone 配置键、稀疏流抵消和缩放、超过 u16 范围的转换 ID，以及结果 JSON 不再输出 variant。原来只测试预留并行接口的用例改为验证实际单流契约。

最终验证：

| 检查 | 结果 |
|---|---|
| `cargo test --workspace --all-targets --no-fail-fast` | **776 通过、0 失败、25 个既有忽略项**；所有 78 个测试目标均结束 |
| 前端 `node --test tests/*.test.mjs` | **136 通过、0 失败** |
| `npm run check` | 0 错误、0 警告 |
| `npm run build` | 生产构建成功 |
| `cargo fmt --all -- --check`、`git diff --check` | 通过 |
| core、runtime、icons 的 `cargo clippy --lib --no-deps -- -D warnings` | 通过；没有放宽告警规则 |
| 最后两处等价 `?` 写法调整后的 core 全目标测试 | 通过 |
| 16 个具体机制展开函数的数字 token 与 HEAD 对照 | 一致；这是辅助静态核对，不替代行为测试 |

日志分别为 `local/baseline-original-rust.log`、`local/final-variant-rust.log`、`local/variant-frontend-{check,tests,build}.log`。显式忽略的压力矩阵和依赖外部游戏环境的长用例未运行，没有将其计作通过。

严格全工作区 Clippy 被未修改的 LP 代码中的参数数量、类型复杂度、map_entry 等告警阻断；shell 还存在较大的 Diagnostic 错误返回值等既有告警。没有提交 allow 属性掩盖这些告警。最终检查应分别记录通过的检查与这些限制。

## 后续达到 20% 所需的调查

目前减少的是局部重复和废弃接口，不能据此推断还存在可安全删除的 1.7 万行。进一步精简需要逐项测量 Factory/Universe 的状态操作与 LP 输入适配、shell 异步任务生命周期、前端作用域处理，以及测试夹具的可合并范围。合并时须保留地点身份、ID 稳定性、撤销重做、过期异步结果守卫和现有后端能力。没有为达到行数比例删除求解后端、测试、诊断工具或游戏机制。

原生构建问题及普通 pwsh 与沙箱的差异另见 [Windows 原生依赖构建排查](windows-native-build.md)。

## 后续去重：reducer 与机制展示

以已提交的 `8ad316d`（tauri-specta 接入）为本轮基线：

- 宇宙机制编辑共用带 project/universe 边界的查找；普通和带地点的表达式/目标项复用按稳定 ID 查找的实现，保留各自错误类型与文本。
- 普通工厂与宇宙工厂共用机制兼容性修复、插件槽位钳制的触发判定；导入的 11 处嵌套 ID 重映射共用同一冲突判定，分配顺序不变。
- 机制编辑器与列表分组共用主要原型映射，集中维护目录种类、名称、品质与本地化命名空间。映射由生成的 Rust 机制联合类型检查完整性。
- 新增跨宇宙机制 ID 不得误命中、失败不得改变文档/历史/ID 分配器，以及销毁物品与销毁地格身份不得混用的回归覆盖。后者保护原有行为，不是已复现的旧版显示错误。

相同 tokei 口径下，本轮从 **90,224 SLoC 降到 90,187 SLoC，净减 37 SLoC**，包含新增回归测试。四个生产源码文件的物理行数净减 104 行；这个数字包含空行和注释，不与 SLoC 混用。

当前总量包含 **2,480 SLoC 自动生成的 IPC 绑定**。tauri-specta 减少了手写类型与封装的维护量，但增加了统计中的生成源码；不能据此宣称全库 20% 的目标已经达到。原始 88,766 SLoC 基线与统计口径继续保留，不重设基线掩盖差距。

本轮验证日志：`local/dedup-regression.log`（完整工作区）、`local/dedup-runtime-final.log`（最后一次表达式查找收敛后的 runtime 复测）、`local/dedup-frontend.log`、`local/dedup-{check,build}.log`、`local/dedup-{labels,fusion}-browser.log`。行数统计保存在 `local/dedup-sloc-{all,before,after}.json`。

## 后续去重：项目快照与可达性缓存

承接上一节的工作树，普通工厂快照现在与宇宙工厂快照一样复用 `ProjectSnapshot`，项目输入只有一份字段定义、构造逻辑和可达性解析实现。两类规划回写共用项目设置、规划偏好、原型仓库实例及可达性代次的检查，各自继续检查目标工厂文档，允许其它工厂的无关编辑。

shell 的普通/宇宙求解和自动规划四条路径共用“成功后回填”逻辑；可达性与产能查询两条路径共用锁外计算和 join 后回填逻辑。六条路径在同一处调用现有 `cache_accessibility_if_current`，保留版本与失效代次检查、失败不回填、锁外计算以及原有任务队列、取消和重试行为。

Rust 源码 API 的变化：`SolveSnapshot` 的项目字段收敛到 `inputs: ProjectSnapshot`，常用只读字段与可达性方法通过 `Deref` 访问。构造该结构体的调用方应改用 `inputs`。这些内部快照不参与 serde IPC；生成绑定一致性检查仍通过。原型仓库和依赖图通过 Arc 共享，项目/工厂文档仍独立克隆，不宣称本次消除了所有文档副本或带来未经测量的性能加速。

新增回归测试验证：失败不回填、文档版本变化拒绝回填、相同 revision 下替换上下文仓库仍拒绝回填，以及当前成功计算使缓存变热；测试同时检查计算期间可获取 runtime 锁。

本阶段生产代码减少 **59 SLoC**，新增守卫测试增加 74 SLoC，所以本阶段总量增加 15 SLoC。与 `8ad316d` 相比，累计从 **90,224 降到 90,202 SLoC，净减 22 SLoC**。不能将生产代码的减少或自动生成文件单独排除后的数字，当作全库 20% 目标已完成。

最终完整工作区回归：**779 通过、0 失败、26 个忽略项，78 个测试目标全部结束**；包含任务复用、并发隔离、过期快照重算、超时/取消、真实资产规划以及生成绑定一致性检查。`cargo fmt --all -- --check` 与 `git diff --check` 通过。本阶段没有调整测试产量。日志：`local/snapshot-final.log`、`local/snapshot-guard.log`。

## 后续去重：浏览器回归生命周期

以 `82c3863` 为本阶段基线。17 个脚本共用 `tests/browser-harness.cjs` 管理 Vite 启动、服务就绪、Chromium 启动及资源关闭。各脚本继续持有页面选项、IPC 夹具、交互和断言；`mechanic-layout.cjs` 保留 seed 与 waitFor 的既有导出。静态 AST 对照确认原有 **344 处断言、页面创建和 seed 调用**保留。

共享夹具保留各脚本端口及浏览器可执行文件配置，支持原有的 preview 和外部 URL 分支。异常路径也关闭浏览器和自己创建的服务，借用外部 URL 时不停止外部服务。

顺序验证时，原版网络脚本长时间停在返回 Cytoscape 集合的 `evaluate` 调用，关闭其测试浏览器后取得了该位置的错误栈。仅用于移动节点或调整视图的三个回调现明确返回 undefined，避免将图集合/核心对象交回 Playwright；节点操作和断言保留。修改后的整份网络脚本验证通过，不把未完成的原版运行当作可比较的性能基准。

相同 tokei 口径下，这 17 个脚本从 **2,036 SLoC 降至 1,895 SLoC**（包含新增共享夹具），净减 **141 SLoC**。全库从 **90,202 降至 90,061 SLoC**。原始全库减少 20% 的目标仍未达到。

验证：迁移的 **17/17 浏览器脚本按既有单脚本方式顺序运行通过**；前端 **142 项单元测试通过**；网络 preview 模式通过；真实浏览器资源探针验证了回调异常、浏览器关闭、所属服务停止以及外部服务保留。依赖真实工程和 Rust 编码桥的 `saved-universe-export-browser.cjs` 保持独立生命周期，未计入这 17 项。

日志：`local/browser-harness-serial.log`、`local/browser-harness-serial/results.json`、`local/browser-harness-unit.log`、`local/browser-harness-lifecycle.log`、`local/browser-harness-preview.log`。原版对照日志为 `local/browser-harness-original-{filter,network,universe}.log`。

## 后续去重：普通/带地点目标文档

以 `a5cf00e` 为本阶段基线。`FlowTarget<F = DualVar>`、`TargetTerm<F = DualVar>`、`TargetExpression<F = DualVar>` 共用三份字段和默认值实现，替代原来的六份 DTO。`LocatedTarget`、`LocatedTargetTerm`、`LocatedTargetExpression` 是使用 `LocatedDualVar` 的具体别名，地点继续属于流身份；JSON 没有增加包装层。

普通外部输入和带地点外部输入保留各自的定义：前者支持默认字段，后者要求调用方提供字段，不能为了减少行数而放宽输入协议。

新增兼容性测试先在原实现上运行通过，再在泛型实现上运行通过，覆盖空目标、表达式及项的默认值和跨地点表达式往返。测试还检查 Schema 字段默认值与实际序列化 JSON 一致。前端别名显式指定 Serialize 阶段的普通流或带地点流；更新后的 `bindings.ts`、`types.ts` 经 TypeScript 类型擦除后的 JavaScript 与本阶段 HEAD 逐字节一致。

同一 tokei 口径下，计入新增测试和重新生成的绑定后，从 **90,061 降至 90,036 SLoC，净减 25 SLoC**。原始全库减少 20% 的目标仍未达到。后续继续审查目标与表达式操作中的重复规则。

验证：完整工作区 **781 通过、0 失败、26 个忽略项，78 个测试目标结束**；包含 MCP 完整 inputSchema、生成绑定一致性和真实资产规划。前端 **142 项单元测试通过**，类型检查 0 错误/0 警告，生产构建通过；格式和 diff 检查通过。

日志与统计：`local/planning-wire-{baseline,generic,schema}.log`、`local/planning-targets-regression.log`、`local/planning-frontend-{check,tests,build}.log`、`local/planning-targets-sloc.json`、`local/planning-types-runtime-equivalence.json`。

## 项目设置补全（2026-11-01）

项目设置新增反应堆邻接数量和轨道运输成本面板，复用既有项目消息、持久化、撤销重做和全部工厂重算路径。反应堆选择器覆盖裂变与聚变，不以当前可达性隐藏原型；同原型不同品质共享估算，单个机制的显式邻接数量仍优先。允许 0–255 的整数，删除覆盖恢复默认值 3；遗留或缺失模组原型的覆盖仍显示内部名称并可移除，也可一次清空全部覆盖。

三个成本系数分别按 kg、堆叠数量和有效变质时间（tick）解释，不随显示时间刻度换算，不改变物理运输系数。允许零、小数和科学记数法；空白、负数及非有限值不会发送。每次修改保留其它系数，等待期间禁用面板操作；失败显示错误并恢复已提交值。组件按项目 ID 重建，消息固定发往操作开始时的项目，旧响应不覆盖新项目的界面。

浏览器回归覆盖选择器添加/取消/重复选择、默认恢复、范围边界、缺失原型、错误恢复、同组字段保留和请求进行中的项目切换。前端 142 项测试通过，类型检查零错误/警告，生产构建通过，并检查了真实浏览器截图的布局。后端既有 `fusion` 与 `orbital_cargo` 用例继续验证估算的原型校验、持久化、撤销和联合规划。验证日志：`local/project-estimates-{browser-final,unit,check-final,build-final,drift}.log`。

完整 Rust 回归的 78 个目标全部结束：780 项通过、26 项忽略，唯一失败为注释更新后的生成绑定不一致。重新生成绑定后，`bindings_are_current` 单独复测通过，合计 781 项通过；没有修改求解用例或放宽断言。完整日志保留初次失败：`local/project-estimates-rust.log`；重新生成和复测分别见 `local/project-estimates-bindings-final.log`、`local/project-estimates-drift.log`。

## 手写生产代码新口径基线

使用同一份 `scripts/count-sloc.py` 和 tokei 14.0.0 重算历史提交，不修改历史源码：

```powershell
python scripts/count-sloc.py --revision 1cea8dc --output local/sloc-original-partition.json
python scripts/count-sloc.py --revision cddcdce --output local/sloc-before-feature-partition.json
python scripts/count-sloc.py --revision 830c4d4 --output local/sloc-feature-commit.json
python scripts/count-sloc.py --output local/sloc-current-partition.json
```

脚本要求 Python、Cargo 和带 JSON 输出的 tokei。独立的 `scripts/sloc-rust` 工具通过 syn 语法树识别 Rust 内联测试模块、测试函数和 `impl` 内的仅测试辅助方法，并排除测试专属外置模块的源码。`cfg(all(test, ...))` 可以排除，`cfg(any(test, feature = ...))` 仍可能用于生产，不能一并排除。字符串或注释中的属性、括号不影响识别。为兼容现有 nightly `pub mut(self)` 语法，只在解析副本中按词法 token 将字段写权限标记替换为空白，保留源码行列；实际计数仍使用原文。遇到未支持的测试属性位置会中止而非静默归入生产。两个工具回归测试覆盖条件判断、伪属性字符串/注释、行号、外置模块、辅助方法和 nightly 字段语法。

仅统计 Git 跟踪及非忽略的第一方源码，目录范围和语言与旧口径一致；生成代码按文件头标记单列。测试目录、浏览器夹具、内联测试归入测试；示例和开发脚本分别列出。空白与注释仍由 tokei 排除。输出包括每个文件的分类，便于复核；`inline_rust_tests` 是测试总量的子集，不再重复相加。

| 固定提交 | 手写生产 | 测试 | 生成 | 示例 | 开发脚本 | 合计 |
|---|---:|---:|---:|---:|---:|---:|
| 原始基线 `1cea8dc` | 51,546 | 31,741 | 0 | 4,276 | 1,203 | 88,766 |
| 功能前 `cddcdce` | 50,260 | 31,849 | 2,447 | 4,277 | 1,203 | 90,036 |
| 新功能 `830c4d4` | 50,382 | 31,964 | 2,447 | 4,277 | 1,203 | 90,273 |

旧口径合计与两份既有历史记录完全一致。新功能增加 122 行手写生产代码和 115 行测试代码；不能把新增功能的增长隐藏为精简收益。新口径的 20% 目标为生产代码最多 **41,236 SLoC**，相较原始基线至少减少 **10,310 SLoC**。后续新增测试和生成绑定不抵消该主指标的减少，但继续公开全部分项。

## 后续去重：前端项目消息封装

在功能提交 `830c4d4` 上，16 个项目设置及规划偏好方法共用类型化的 `sendProjectAction`，继续经过既有 `send` 管理刷新、历史和错误；各方法仍返回 `Promise<void>`。共享入口在首次挂起前固定项目，缺少选中项目时仍返回拒绝的 Promise，后端拒绝对象保持原样传播。纯委托方法直接返回共享入口的 Promise，避免再增加一层 async 包装。

对照重构前版本执行了 16 个方法的差分探针，验证消息、延迟请求期间的项目切换、void 返回、错误对象身份和缺失选择时的异步拒绝。另有 142 项前端测试、类型检查、生产构建，以及项目估算和项目文件两份真实浏览器回归全部通过。本阶段不修改 Rust、生成绑定或求解算法，不宣称未经测量的性能加速。日志：`local/project-actions-{equivalence,unit,check,build,estimates-browser,files-browser}.log`。

本阶段手写生产代码净减 **59 SLoC**，当前 **50,323 SLoC**；较原始 **51,546** 净减 **1,223（约 2.37%）**，距离 20% 还需减少 9,087 行。新增统计工具独立列入开发脚本，其回归测试列入测试。

| 当前分项 | SLoC |
|---|---:|
| 手写生产 | 50,323 |
| 测试 | 32,010 |
| 自动生成 | 2,447 |
| 示例 | 4,277 |
| 开发脚本 | 1,518 |
| 合计 | 90,575 |

分类明细：`local/sloc-final-partition.json`。本轮审阅也确认普通/宇宙目标表达式操作的 ID 分配和错误契约仍有差异，不能直接用同一分支替换；未为了减少行数改变其行为。

## 轨道运输默认值调整

按最新实测结论，火箭发运的每 kg 系数和变质额外成本系数改为 **64³ = 262,144**，轨道投送的每组常数保留 **64**。新建项目和缺失字段使用新默认值；已保存的显式值（包括旧默认值和零）不迁移、不覆盖。前端运输提示与项目估算面板共用回退常量，避免两处独立维护。

浏览器回归复现了上一阶段的一个边界遗漏：前端旧式数据缺少整组 `orbital_transport_costs` 时，新增面板读取属性会抛错，导致项目设置不可见。现在面板对缺失字段使用回退值，仅查看不会写入；第一次编辑会带上完整的其它成本字段。普通 Rust 文档的 serde 默认补齐逻辑保持不变。增加浏览器覆盖验证该行为以及显式旧值/零值保留。

最终 64³ 默认值的 `orbital_cargo` **9 项测试通过**，覆盖重量与载荷、双向运输、变质品质倍率、完整规划、回写成本、持久化、撤销和拒绝非法成本；没有调整目标产量。大目标成本的比较使用四倍机器精度的舍入容差，其它物理系数和数值断言继续保留。前端 **142 项测试通过**，类型检查零错误/警告，生产构建与项目设置/宇宙工作区两份浏览器回归通过。

日志：`local/orbital-defaults-64cube.log`、`local/orbital-defaults-{check,unit,build}-final.log`、`local/orbital-defaults-{estimates,universe}-browser-final.log`；修复前的缺失字段错误保存在 `local/orbital-defaults-legacy-before.log`。中途被新要求替代的 2²⁴ 运行已停止，不计作最终版本验证。

## 后续去重：规划报告回填

普通工厂同步规划、shell 异步规划及宇宙工厂规划的报告回填收敛到 runtime。两类结果共用候选数量、回写阈值和完整候选规划证据的更新；普通工厂仍使用候选 LP 的变量数量/后端，宇宙工厂保留回写后重解的变量数量/后端。回写子问题的最优性、相对误差及精确验证证据不被规划报告覆盖。

报告按值交接，规划优化报告及其中的 pivot 进度向量直接移动，移除原有四个调用位置需要的克隆；未运行时间基准，不宣称具体加速比例。Rust 的 `attach_universe_plan_report` 现在接收拥有所有权的 `AutoPlanReport`，原有借用调用方应传入报告本身；若还需保留则由调用方显式克隆。IPC 类型和 JSON 字段不变。

原 shell 的候选超时证据测试移动到 runtime，并扩展为普通工厂/宇宙工厂对照：两条路径都保留回写证据和规划超时，宇宙工厂保持原有维度与后端。测试没有删除，完整回归日志为 `local/plan-report-final-regression.log`。

最终工作区 **781 项 Rust 测试通过、0 失败、26 项忽略，78 个目标全部结束**；包括最终 64³ 默认值、报告证据对照、真实资产规划及生成绑定一致性。前端 **142 项测试**、类型检查、生产构建、项目估算及宇宙工作区浏览器回归全部通过，Rust 格式和 diff 检查通过。

新口径统计：默认值与缺失字段兼容阶段从 **50,323 增至 50,332**；本去重阶段从 **50,332 降至 50,321，净减 11 SLoC**。本轮合计净减 2 行；较原始 **51,546** 累计减少 **1,225（约 2.38%）**，20% 目标仍未达到。

| 当前分项 | SLoC |
|---|---:|
| 手写生产 | 50,321 |
| 测试 | 32,068 |
| 自动生成 | 2,447 |
| 示例 | 4,277 |
| 开发脚本 | 1,518 |
| 合计 | 90,631 |

固定的默认值阶段明细为 `local/orbital-defaults-committed-sloc.json`，当前明细为 `local/plan-report-sloc.json`。
