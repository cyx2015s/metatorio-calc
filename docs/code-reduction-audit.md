# 代码精简与性能审查（2026-10-28）

本轮已落地可验证的去重与性能改动，**尚未达到整个代码库 SLoC 减少 20% 的目标**。统计不包含第三方 vendor、游戏 JSON 数据、构建输出和 Markdown，不以删除测试或压缩代码排版计算收益。

## 统计口径

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
