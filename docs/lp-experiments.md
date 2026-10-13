# lp-exp：原模型校验与支持集重求实验

本地分支 `lp-exp`。目标是建立可以重复验证的难例，发展 Rust 的数值修正、证书和搜索内核，最终替换当前应用求解路径。现已有独立 Rust 精确 LP 实验内核，全量难例稳定性和生产集成仍在推进。

阶段更新：现已提取独立的 `metatorio-lp` Rust crate，支持精确两阶段求解和基更新；默认应用后端仍是 HiGHS。以下各阶段保留当时的测量和适用范围，最新状态见第三阶段。

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

## 第二阶段：Rust 精确基与精确单纯形（2026-10-13）

新增 `rational-lp` 功能和 `lp_rational` 模块。这个核心没有 HiGHS 调用：

- 使用纯 Rust 大整数/有理数重新构造基础矩阵，消去基本行松弛变量，精确求出原始解和行对偶。
- 检查每条原始行、每个变量边界，对完整候选列进行精确定价；只有原始可行性、对偶可行性及目标差严格为 0 时标记最优。
- 全量定价使用共享分母整数运算，避免每个矩阵乘积都约分；覆盖最小非零浮点、最小正常数、极大值和普通小数的独立有理数对照测试。
- 实现针对应用当前变量形式的精确原始单纯形：变量下界 0、无有限上界，等式与 `<=` 行；用 Dantzig 选列，检测到重复基后永久切换到 Bland 选列。
- 返回无界前独立验证精确改善射线。换基上限或外部停止保留当前可行解、基和非终止原因，可以继续求解，不转换为不可行。

输入语义是捕获的 `f64` 模型的**精确二进制有理数值**，不是从游戏原始数据恢复的十进制或整数比例。最终转回 `f64` 时仍有舍入；精确证书对应保存的有理数解。`metatorio-solver` 目前整体仍依赖 good_lp/HiGHS，Rust 核心本身不调用它们；独立求解 crate 的提取仍待完成。

### 精确检查改变了上一轮结论的适用范围

24 个冻结模型全部做了精确基重构与原模型检查：

| 检查层次 | 通过记录数 |
|---|---:|
| 上一轮浮点预算下可行 | 24/24 |
| 这组候选基重构后的严格原始可行 | 15/24 |
| 这组支持子问题原始/对偶严格最优 | 15/24 |
| 这组支持解的行对偶证明完整模型最优 | 0/24 |

剩余 9 个候选基存在严格意义上的行或边界违反；这只否定这些候选基，不证明模型不可行。其余 15 个原始解严格可行，但支持子问题的这组对偶未覆盖完整模型的改善列，也不证明原始解不是全局最优。完整汇总为 `local/lp-exp/exact-summary.json`。

共享分母优化前后，首批样本的精确判定和改善列计数保持一致：史诗级生物室完整检查从 39.26 秒降至 0.16 秒，传奇级堆叠机械臂从 269.51 秒降至 0.56 秒。时间来自本机实验日志，包含相同原模型定价，基础矩阵重构时间另算。

### 第一个完整模型的 Rust 精确最优证书

对请求 4（稀有品质插件塔设置、Fulgora、普通电磁工厂目标）的 **2,032 变量、373 行完整模型**运行 Rust 精确单纯形。初始候选基来自上一轮已保存的 22 列支持 LP，这次运行没有调用 C++ 求解：

- 仅 Bland 选列在 120 秒内执行了 422 次换基，尚未收敛；该实验报告非终止，未报不可行。
- Dantzig + 重复基检测退路执行 99 次换基得到完整模型精确最优证书，初测 19.13 秒、最终版复跑 30.27 秒，本例没有触发 Bland 退路。
- 完整原始约束和边界严格满足，对偶符号严格满足，精确目标差为 0。
- 转成 `f64` 后目标值约 `3988.917887719224`，通过原应用行校验且无边界违反。
- 另有经典退化循环样例实际触发重复基检测/Bland 退路，并得到精确最优解；无界射线、有限上界证书、微小缺口和非终止恢复也有测试。

证据位于 `local/lp-exp/corpus-20261013/replay-support-refine-20261013-084337/request-4/rust-simplex-final.json`；该文件保留完整基、精确非零原始解、行对偶和原模型证书。

从该文件的基重新启动后，0 次换基、约 0.30 秒再次通过完整最优证书，记录为同目录的 `rust-simplex-resumed.json`。

```powershell
$env:CMAKE_GENERATOR = 'Ninja'
cargo build --release -p metatorio-solver --features lp-experiments --example lp_certify
$cases = Get-Content local/lp-exp/corpus-20261013/request-outcomes-0.jsonl |
    ForEach-Object { $_ | ConvertFrom-Json }
$caseDir = 'local/lp-exp/corpus-20261013/replay-support-refine-20261013-084337/request-4'

# 只重构已保存的基，并检查完整原模型。
./target/release/examples/lp_certify.exe $cases[4].snapshot_paths[0] `
    "$caseDir/support-refine.json" "$caseDir/exact-check.json"

# 由 Rust 在完整候选列上换基，例程提供 120 秒非终止运行限额。
./target/release/examples/lp_certify.exe $cases[4].snapshot_paths[0] `
    "$caseDir/support-refine.json" "$caseDir/rust-simplex-final.json" --solve

# 从保存的完整基继续；已经最优的基应以 0 次换基重新通过证书。
./target/release/examples/lp_certify.exe $cases[4].snapshot_paths[0] `
    "$caseDir/support-refine.json" "$caseDir/resumed.json" `
    --resume "$caseDir/rust-simplex-final.json"
```

这是候选基与精确核心分层的起点。[SoPlex 的官方精确 LP 说明](https://soplex.zib.de/doc-7.0.0/html/EXACT.php)也介绍了浮点搜索与精确求解结合的方向；本原型尚未实现其迭代修正和精度提升能力。

### 仍需完成

该阶段结束时，下一步是 Phase I/双重单纯形修复、分解复用和独立 crate 提取。其后的进展见下节；整个特化 Rust 求解器目标仍未完成。

## 第三阶段：独立 crate、从零启动和精确基更新

新增工作区成员 `crates/metatorio-lp`，依赖仅为 Rust 大整数、有理数和 serde；依赖树没有 HiGHS、good_lp 或 C++ 构建库。在受限环境中直接运行 `cargo test -p metatorio-lp`，12 项核心测试全部通过。`metatorio-solver::lp_rational` 保留为重导出，默认应用行为未修改。

`Model::solve` / `solve_with_observer` 从零构造人工变量并执行 Phase I。正的辅助最优值必须对应原始模型可独立验证的 Farkas 证书：不等式行 `y <= 0`、全部列 `A^T y <= 0` 且 `b^T y > 0`。验证不使用原始目标成本。辅助目标归零后清理人工变量，执行原始目标的 Phase II。冗余等式、空的不可行行、负 RHS、无界目标和非终止限额均有测试。

加入精确压缩基逆更新，覆盖结构列替换、基本行松弛变量替换、扩维和降维。每一步仍进行原始模型的精确证书检查；四类更新另与独立完整 Gaussian 重构比较原始解和行对偶，结果严格相同。更新复用消除了每次换基三次重新分解的主要成本，但仍是稠密有理数原型，结构基维数上限为 256。

此前已严格最优的 2,032 变量完整模型经新基更新核心复跑，仍为 99 次换基、完整证书目标差为 0，时间降至 3.45 秒。记录为 `request-4/rust-simplex-cached.json`。该测量仍采用保存的初始基。

对上一阶段 9 个严格校验失败的样本，**只读取支持列索引，不读取候选基或浮点解**，从零执行 Rust 两阶段求解，单例总限额为 45 秒：

| 请求编号 | 结果 | Phase I 换基 | Phase II 换基 | 时间（秒） |
|---|---|---:|---:|---:|
| 2 | 未完成 | 304 | — | 45.04 |
| 7 | 未完成 | 348 | — | 45.08 |
| 8 | 支持模型精确最优、完整模型严格可行 | 145 | 6 | 2.53 |
| 10 | 未完成 | 194 | — | 45.51 |
| 12 | 支持模型精确最优、完整模型严格可行 | 59 | 49 | 1.97 |
| 13 | 支持模型精确最优、完整模型严格可行 | 36 | 26 | 1.92 |
| 18 | 未完成 | 336 | — | 45.25 |
| 19 | 支持模型精确最优、完整模型严格可行 | 276 | 4 | 8.70 |
| 20 | 支持模型精确最优、完整模型严格可行 | 80 | 10 | 3.56 |

完整原始约束一直保留，新增的 5 个严格可行解以原始列索引保存有理数。结合上一阶段 15 个严格可行样本，现在 20/24 条记录已有严格可行解；这 5 个支持模型的对偶仍未证明完整模型最优。其余 4 个均为运行限额后的未完成状态，没有被宣称不可行。汇总为 `local/lp-exp/phase1-summary.json`，单例为原回放目录下的 `phase1-updated.json`。

对 2,032 变量完整电磁工厂模型，也直接从零运行，没有读取支持列或候选基。旧版重分解在 120 秒内只执行到 Phase I 的 190 余次换基；基更新后同样限额内执行到 930 余次，仍未完成。准确的非终止结果见 `request-4/full-from-zero-updated.json`。已有完整最优证书来自此前的热启动；不能用它宣称该模型从零启动已成功。

```powershell
cargo test -p metatorio-lp
cargo clippy -p metatorio-lp --all-targets -- -D warnings
cargo tree -p metatorio-lp
cargo build --release -p metatorio-solver --features lp-experiments --example lp_from_zero

# 不提供任何候选基；支持文件只读取 selected_columns。
./target/release/examples/lp_from_zero.exe $cases[8].snapshot_paths[0] `
    "$caseDir/from-zero.json" "$caseDir/support-refine.json" 45

# 完整候选模型从零启动，完全不读取旧支持集。
./target/release/examples/lp_from_zero.exe $cases[4].snapshot_paths[0] `
    "$caseDir/full-from-zero.json" full 120
```

下一阶段优先实现精确双重单纯形：那 9 个原始候选基的支持模型行对偶均严格可行，因此可以从对偶侧快速修复微小原始违反。同时改进 Phase I 的退化处理/候选列构造和稀疏基分解，解决完整模型冷启动成本，再逐步接入真实生产回写。独立 Rust crate 已存在，全部难例稳定完成和生产接入仍未完成。
