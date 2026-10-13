# lp-exp：原模型校验与支持集重求实验

本地分支 `lp-exp`。目标是先建立可以重复验证的难例，再发展 Rust 的数值修正与证书层，逐步替换求解内核。当前实验仍用 HiGHS 搜索与重求；尚未实现纯 Rust LP 求解器。

## 已建立的实验入口

- `metatorio-solver` 的 `lp-experiments` 功能：捕获原始目标、连续变量边界、约束、变量/行标签与生产 RHS 缩放。浮点值按 `u64` 位模式保存，支持无限边界。
- `METATORIO_LP_CAPTURE_DIR` 指定输出目录。每次求解使用独立文件，捕获失败只记录错误，不改变求解结果。
- 独立回放工具：保存解、对偶、基状态、后端状态、加载后非零系数数目、配置和迭代数。
- 原问题验证：补偿乘积与求和、变量边界、原始行活动量；对偶检查包含驻点、符号、互补条件和目标差。这些属于浮点诊断，尚不是精确证书。
- 24 个历史请求已固定在 `crates/metatorio-shell/src/mcp/auto_plan_assets_tests/fixtures/lp-failure-requests.jsonl`。诊断采集测试会记录真实 MCP 流程的成功/失败；测试通过只表示采集成功。成功请求另有回写与同步重求断言。

默认功能下的生产求解算法、重试条件、容差和回写行为未调整。实验重求结果尚未接入生产流程。

## 第一轮结果（2026-10-13）

数据源为本地 `assets/data-raw-dump.json`，链接的 C++ 后端为 HiGHS 1.15.0。上游源码克隆位于 `playground/HiGHS`；演进和 AI 重写案例研究位于 `playground/highs-research/RESEARCH.md`。

24 条历史请求记录重跑后，真实流程完成 2 个、失败 22 个。其中有一条重复请求，共 23 个不同请求；保留重复项以追踪报告来源。成功数会受到变量枚举顺序和数值路径影响；所以后续比较使用冻结矩阵，而不是每次重新构造游戏模型。

冻结模型从 2,032 到 163,398 个变量。默认路径中，20 个返回 Optimal 但未通过原模型行校验，4 个返回 Unknown。失败样本反复出现核燃料供应行缺口；Unknown 的候选还可能违反非负边界。Unknown、超时及未经原模型校验的 Optimal 均不可当作不可行或已解出的结论。

史诗级生物室的冻结 LP（131,249 变量）：

| 路径 | 状态 | 原模型行校验 | 备注 |
|---|---|---|---|
| 默认 | Optimal | 最大相对违反 1 | 核燃料供应缺口约 4.88e-8 |
| 关闭缩放 | Optimal | 通过 | 目标值约 74285.61487397 |
| 关闭缩放及 presolve | Optimal | 通过 | 同一目标值 |
| 收紧容差、保留更小系数 | Optimal | 失败 | 核燃料缺口约 1.95e-7 |
| 收紧容差、关闭 presolve | Optimal | 通过 | 同一目标值 |
| 不放大 RHS | Optimal | 通过 | 同一目标值 |
| IPX + crossover | Optimal | 失败 | 收紧容差也没有消除缺口 |
| IPX、不做 presolve | 超时（20 秒） | 失败 | 不代表不可行 |
| 支持集重求 | Optimal | 通过、边界违反 0 | 131,249 → 76 变量 |

缺口对应的虚拟燃料流约 4.88e-8，同时连接真实燃料流约 1.22e-15；后者乘以约 4e7 的系数。供应行的几个核反应堆变量为 0，系数约 -8e9。这里的小变量有跨行连接，不能仅凭数量级删除。单列零成本归零实验在真实样本上没有修复这一问题。

## 支持集重求的定义与限制

1. 从完整模型取得候选解。未终止时尝试关闭缩放和 presolve；优先选择 Optimal，否则保留原模型行/边界违反较小的有限候选。候选只用于选择变量。
2. 保留所有精确非零的候选变量；没有按流量大小设截断阈值。
3. 保留不能取 0 的变量，以及未通过原模型行校验的每一行中的所有变量，让缺失的供应方可以进入子问题。
4. 其余变量固定为 0，所有原始约束保留，重求连续 LP。
5. 将解映射回完整模型，重新检查原始行、边界、有限性与目标值。

最终统一回放中，24/24 个冻结模型均返回 Optimal，完整原模型的行检查全部通过，边界最大违反均为 0；支持集为 22–208 变量。汇总位于 `local/lp-exp/corpus-20261013/replay-support-refine-20261013-084337/summary.json`。

最后一个传奇级堆叠机械臂模型也体现了路径差异：关闭缩放并关闭 presolve 报 Unbounded，但其有限候选的行/边界误差较小；不放大 RHS 的完整模型路径返回 Optimal 且通过数值校验。使用更好的有限候选构建 133 列支持子问题后，返回通过原模型校验的解。支持重求的目标值约 1060533.407557，另一完整模型路径约 1060532.999503；这项差异也说明当前结果只能用于可行性修复实验，还需原模型定价与精度修正来评估最优性。

这里的行预算沿用生产判断 `abs <= 1e-6 * max(sum(abs(a*x)), abs(b)) + 1e-8`，此外实验汇总要求边界最大绝对违反不超过 1e-8，状态为 Optimal，所有活动量有限。通过意味着符合这组数值预算，不是数学意义上精确满足约束。限制子问题的最优值也不证明原模型全局最优；工具明确保存 `global_optimality_verified: false`。

`small_matrix_value` 必须在 `Highs_passLp` 之前设置，否则系数在载入时已被删除。回放现在先配置空模型再载入 CSR，并记录加载后的非零数。较低的 1e-12 阈值仍可能删除更小系数，因此必须验证原始矩阵。

## 重现

PowerShell 7，在仓库根目录执行。需要本地资产以及能够编译 highs-sys 的 C++/CMake 环境；本机使用 Ninja。

```powershell
# 采集 24 个真实请求，然后对冻结矩阵运行支持集实验。
./scripts/lp-experiment.ps1

# 重用已采集的模型，结果写入独立带时间戳的目录。
./scripts/lp-experiment.ps1 -ReplayOnly `
  -CorpusDirectory D:/rust-projects/metatorio/local/lp-exp/corpus-20261013 `
  -Strategy support-refine -TimeLimit 20

# 比较完整模型的另一条数值路径。
./scripts/lp-experiment.ps1 -ReplayOnly `
  -CorpusDirectory D:/rust-projects/metatorio/local/lp-exp/corpus-20261013 `
  -Strategy no-scaling

cargo test --release -p metatorio-solver --lib
cargo test --release -p metatorio-solver --features lp-experiments --lib
cargo clippy -p metatorio-solver --features lp-experiments --all-targets -- -D warnings
cargo check --release -p metatorio-shell --features lp-experiments --tests
```

脚本记录资产/请求/矩阵 SHA-256、Git 状态、编译器版本，退出时恢复环境变量。新的捕获目录必须不存在，避免覆盖证据。大矩阵和结果写到被忽略的 `local`；源码、请求夹具和本文纳入 Git。

## 下一阶段的验收顺序

1. **完整流程修复实验**：将支持集候选接入 opt-in 求解路径，保留原模型验证，跑所有 24 个请求的真实回写与同步重求；再覆盖正常、不可行、小目标和重模组样本。不能把冻结 LP 的通过数当作完整流程通过数。
2. **Rust 精度修正层**：读取小子问题的基，重构基础矩阵，使用更高精度/有理数重求原始解与对偶；退化或奇异时换基。优先利用现在几十到两百列的子问题，避免直接对十几万列做高成本运算。
3. **原模型定价与证书**：对完整候选列计算 reduced cost；存在改善列时扩展支持集。只有完整原始矩阵的可行性、对偶可行性和目标界共同成立，才升级为已验证最优。
4. **独立 Rust 搜索内核**：逐步实现稀疏存储、基分解/更新、修正单纯形、退化处理和可逆 presolve。每替换一个模块，都运行冻结矩阵差分；保留 HiGHS 作参照。

首个更高精度版本的成功条件应是：目标样本完成真实回写，数值不确定与不可行分开报告，保留必要的微小流量，并提供可审查的残差/证书。更强大的 LP/QP/MIP 功能在这个基础稳定后逐步扩展。
