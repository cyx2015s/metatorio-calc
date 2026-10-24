# 线性规划后端算法调研（2026-10-24）

## 0. 结论摘要

- 本项目需要的不是“通用 LP 目标值”，而是**稀疏顶点支撑集 + 高精度目标 + 可写回工厂**。这决定了单纯形/基方法比内点法/一阶法更合适。
- HiGHS 的 presolve、缩放、对偶单纯形、稀疏基和速度是成熟实现，应直接采用，不应重写。
- Rust based 的价值在精确二进制有理数重构、完整原模型证书、Farkas 和精确零；但 route/anytime/relevance 这些启发式应剥离。
- 推荐的单用户后端形态：**HiGHS 找候选基 → Rust 精确重构/认证/修复 → 失败再回退 Rust 全搜索**。如果“单后端 + 精确性优先”，优先实测 **SoPlex exact**；如果必须纯 Rust，则借 HiGHS 的算法思想（presolve、缩放、Harris、bound flipping、稀疏 LU/FT），保留精确终检。
- **只缩放目标/ RHS 不够**：HiGHS 默认 `small_matrix_value` 约 `1e-9`（最小约 `1e-12`），Gurobi 约 `1e-13`；`1e-24` 的必要系数可能在输入阶段就被当零。必须一致缩放模型并保留精确输入语义。
- Clarabel（无 crossover 的内点法）不适合自动规划写回；PDLP（一阶法）通常不直接给顶点；microlp 适合小模型/验证，不适合百万列主求解。

## 1. 需求摘要

| 维度 | 当前项目的事实 |
|---|---|
| 规模 | 约 1.18M 列、3.5k 行、7.3M 非零；极端案例更大 |
| 变量 | 连续、非负、无有限上界；成本非负 |
| 行 | 等式为主，另有 `<=`；目标行常是唯一非零 RHS |
| 系数 | 可跨 20 个数量级（约 `1e-24` 到 `1e14`） |
| 目标 | 目标物品产量约束下的最小成本；需要高精度目标 |
| 输出 | 约 50–300 个非零机制的顶点支撑集，用于写回工厂 |
| 数值 | 1/s 小目标下必需流量可能极小；绝对容差容易丢列 |
| 运行 | 交互/自动规划，需秒到分钟级；支持超时和取消 |
| 验证 | 精确可行性/最优性或至少写回子集可解；Farkas 不可行证明 |
| 工程 | 可选 native-free 纯 Rust；项目级后端选择；错误/超时原样暴露 |

当前的管线问题在 [lp.rs](crates/metatorio-solver/src/lp.rs:295)、[auto_plan.rs](crates/metatorio-runtime/src/auto_plan.rs:63)、[solve.rs](crates/metatorio-runtime/src/solve.rs:791)：HiGHS 直接求解后按 `amount != 0` 取支撑集；Rust 走 staged float + exact cert；自动规划的“移除未用”本身会重新求解。

## 2. 已有实现的算法

| 技术/算法 | 已有实现（代表） | 状态 | 对本项目的价值 |
|---|---|---|---|
| Presolve | HiGHS、Clp、Gurobi、CPLEX、GLPK、SoPlex | 生产实现 | 必须；减少列/行、发现不可行、改善数值 |
| 均衡缩放（equilibration） | HiGHS、Clp、Gurobi、CPLEX、SoPlex | 生产实现 | 必须；但只缩放 RHS 不够，要一致缩放模型 |
| 原始/对偶单纯形 | 所有主流 LP solver | 生产实现 | 必须；顶点/基/稀疏支撑集的来源 |
| Harris 两遍比值检验 | HiGHS、Clp、商业 solver | 生产实现 | 必须；抗退化、避免近奇异主元 |
| Bound flipping ratio test | HiGHS、Clp 等对偶单纯形 | 生产实现 | 高价值；长列/退化模型减少 pivot |
| Steepest edge / Devex | HiGHS、Clp、商业 solver | 生产实现 | 可选；改善 pivot 质量，成本更高 |
| Forrest–Tomlin 更新 | HiGHS、Clp、SoPlex | 生产实现 | 高价值；替代每步重分解 |
| 稀疏 LU / Markowitz | HiGHS、Clp、SoPlex、商业 solver | 生产实现 | 必须；大模型基矩阵求解的核心 |
| 迭代精化（iterative refinement） | 研究论文 + 部分 solver 实现 | 研究/部分实现 | 高价值；HiGHS 候选转精确前的中间层 |
| Crossover（IPM → 顶点） | Gurobi、CPLEX、HiGHS（`run_crossover`） | 生产实现 | 需要；Clarabel 没有；PDLP 通常没有，Gurobi PDHG 例外 |
| 内点法/barrier | HiGHS、Clp、Gurobi、CPLEX、GLPK | 生产实现 | 可做对照/下界；不直接给稀疏顶点 |
| PDLP / PDHG | OR-Tools PDLP | 生产实现（一阶法） | 大规模候选；不直接给基/顶点，精度和写回不匹配 |
| 精确有理数单纯形 | SoPlex exact、QSopt_ex | 专用生产/研究实现 | 高价值；小/中模型终检、Farkas、精确零 |
| 列生成 | 大型 MILP/B&P、部分 LP 分解工具 | 生产实现（分解场景） | 若枚举列是瓶颈；需要全局定价与完整证书 |
| Farkas 证书 | 主流 solver 的不可行状态/精确工具 | 生产实现 | 必须；区分“不可行”和“算不动” |
| Warm start / basis | HiGHS、Clp、商业 solver | 生产实现 | 高价值；重复规划/写回重算 |

### 关键事实

- HiGHS 1.14 的 `solver` 支持 `simplex` / `ipm`，`run_crossover` 默认 **off**；simplex 直接给基/顶点。[HiGHS options](https://ergo-code.github.io/HiGHS/dev/options/definitions/)、[HiGHS manpage](https://manpages.ubuntu.com/manpages/stonking/man1/highs.1.html)
- OR-Tools 明确说：simplex 返回顶点；barrier/一阶法通常不返回顶点；crossover 可以把 barrier 解转到顶点，但**一阶法目前不提供 crossover**。[OR-Tools Advanced LP](https://developers.google.com/optimization/lp/lp_advanced)
- SoPlex 有 exact rational 模式；QSopt_ex 是基于 GMP 的有理数 LP 求解器。[SoPlex exact](https://soplex.zib.de/doc-7.1.4/html/EXACT.php)、[QSopt_ex](https://www.math.uwaterloo.ca/~bico/qsopt/ex/)
- PDLP 的论文强调一阶法可扩展、可达高精度；但它不是基方法，不能直接提供本项目的机制支撑集。[PDLP paper](https://arxiv.org/abs/2106.04756)

## 3. 纯理论/研究原型

| 方法 | 代表工作 | 为什么不能直接用 |
|---|---|---|
| 随机化单纯形 | Kelner–Spielman, *A randomized polynomial-time simplex algorithm for LP* | 理论多项式期望时间；没有可比的生产实现/工程常数 |
| 强多项式 LP | Tardos 等 | 理论复杂度结果；不提供可用 solver |
| Smoothed analysis | Spielman–Teng | 解释单纯形行为；不是算法实现 |
| 低维随机 LP | Seidel 等 | 维度指数；不适合百万列 |
| ADMM for LP | 研究论文/原型 | 收敛慢、精度有限；OSQP 是 QP，不是精确 LP |
| 学习型 presolve/pricing | 多篇研究 | 依赖训练分布；难保证可行性/最优性 |
| 量子 LP | 理论/实验 | 不适用于当前工程 |

> PDLP 已经进入 OR-Tools，属于“已实现”的一阶法，不是纯理论；但它和本项目的“顶点 + 基 + 写回”需求不匹配。
> 精确有理数单纯形也不是纯理论：SoPlex/QSopt_ex 已实现，只是规模/性能有限。

## 4. 对本项目的缺口

1. **目标尺度归一化**：当前 HiGHS 路径不再做 RHS 缩放；1/s 时小流量被绝对容差抹掉，10000/s 才正常。需要一致的归一化（目标行/变量缩放，或 power-of-two 规范化），并在缩放后回验。
2. **写回子集可解性**：HiGHS 的 95 个非零变量写回后不可行，说明“非零支撑集”不等于“可行写回集”。应加轻量写回检查；不是恢复完整证书，而是检查输出语义。
3. **精确终检**：HiGHS 基可以用 Rust 精确重构/认证；这是“HiGHS 速度 + Rust 精度”的关键接口。
4. **Clarabel 状态**：good_lp 的 Clarabel 适配把 `AlmostSolved` / `DualInfeasible` 当 `Ok`，并总是报告 `Optimal`；不能用于自动规划主路径。
5. **Rust 启发式层**：route/anytime/relevance/surplus 应剥离；保留精确内核和支撑集修复。

## 4.5 Codex 并行调研补充的关键约束

这轮并行调研返回后，有几条会直接改变方案判断：

1. **如果模型确实是 `c >= 0, x >= 0` 且可行，目标不可能无界。** 目标下界至少为 0；可行域无界、零成本射线、目标无界是三件不同的事。后端报 `Unbounded` 时，优先检查目标方向、行列转换、符号或数值问题，而不是直接接受。
2. **基本可行解的非零变量数最多约为行数 m（这里约 3.5k），不会自动保证只有 50–300 个原变量非零。** 50–300 是模型结构、退化性和支撑集选择的结果，不是“单纯形必给”的保证。写回前应区分 **basis** 和 **support**：基本变量也可能因退化为 0。
3. **宽矩阵本身值得单独优化。** n/m ≈ 286，每列平均约 7 个非零；百万列主要压定价、扫描和存储，而不是基矩阵。sifting、部分定价、工作列集/列生成值得优先评估；但列生成停止前必须对全部遗漏列定价。
4. **只缩放目标行不够：输入阶段就可能丢系数。** HiGHS 的 `small_matrix_value` 默认约 `1e-9`，最小可设约 `1e-12`；小于等于阈值的矩阵项会被视为零。Gurobi 数值指南也说明约 `1e-13` 以下的系数会被当作零。当前模型里 `1e-24` 量级的必要系数，**不能靠“把目标放大到 1e4”恢复**；要么在输入/缩放阶段保留精确有理数，要么对矩阵做一致的行列缩放并证明没有删项。
5. **正确的缩放是 `A' = D_r A D_c, b' = D_r b, c' = D_c c`，同时映射变量界、对偶变量、残差和目标单位。** 只改 RHS、不改同行系数，会改变问题；把整行除以极小 RHS 又可能制造巨大系数。当前“只缩放 RHS”的旧做法不是通用解。
6. **SoPlex exact 是最接近“单后端 + 精确认证”的现成实现。** 它有浮点搜索、LP iterative refinement、precision boosting、rational LU/reconstruction、exact parsing/checking。若精确性优先且能接受原生依赖，应优先实测 SoPlex exact；不能先把输入转成 f64 再转有理数，否则认证的是另一组数据。
7. **精确检查器的核心条件很小。** 标准形 `min d^T z, Mz=b, z>=0`：
   - 最优：`Mz=b, z>=0, M^T y <= d, d^T z = b^T y`；
   - 严格 gap：primal/dual 可行且 `0 <= d^T z - b^T y <= epsilon`；
   - Farkas：`M^T y >= 0, b^T y < 0`。
   这些可以用有理数直接验证；普通高精度浮点重算不等于证明。
8. **没有有限上界时，微小负 reduced cost 不能给出有限下界。** 对 `z_j >= 0` 无上界的列，`r_j = d_j - M_j^T y < 0` 即使只有 `-1e-15`，也可能让拉格朗日下界无效。PDLP 文档区分“报告的 dual objective”和“corrected dual objective”，后者可能是 `-inf`；报告的 gap 小不等于严格 `1e-12` 最优性。
9. **PDHG 的 crossover 不能一概而论。** OR-Tools PDLP 没有标准基/crossover；HiGHS PDLP 也没有标准基输出；但当前 Gurobi 文档已经列出 PDHG + crossover。不要把“PDLP 没有 crossover”外推到所有 PDHG 实现。
10. **状态和建议分开保存。** 例如 `TimeLimit + VerifiedFeasible`、`Optimal + FloatChecked`、`Optimal + ExactCertified`。超时、循环、数值失败、认证未完成都不能映射成 infeasible。

## 5. 单后端/混合架构建议

### 方案 0：如果“单后端 + 精确性优先”，先测 SoPlex exact

SoPlex 的浮点搜索、LP iterative refinement、precision boosting、rational LU/reconstruction 和 exact parsing/checking 在同一个实现里，最省去自行组合“浮点找基 + 有理数认证”的工作。代价是原生依赖、精确百万列扫描和大整数膨胀仍需实测；输入必须保持有理数语义，不能先转 f64 再转 rational。

本地已接入 **patched `vendor/soplex-sys`**（`soplex` feature）：补丁修正 C++17、MSVC `/EHsc` 和 Windows `libsoplex.lib` 链接名；适配器把模型写成稀疏 MPS，走 SoPlex 的 file reader。当前是**浮点 SoPlex，不是 exact rational 模式**；精确模式仍需带 GMP 的原生 SoPlex 或独立精确检查器。

### 方案 A（推荐，速度+精度）：HiGHS 候选 + Rust 精确终检

1. HiGHS + presolve/scaling/dual simplex 解全模型，拿 basis/vertex。
2. 用 Rust 精确有理数重构该基。
3. 精确 `certify`：
   - 通过 → 返回；
   - 目标有差距 → 精确原成本优化/修复；
   - 支撑集不可行 → Rust 支撑集修复；
4. 精确终检失败 → Rust 全搜索回退。
5. 输出前做写回子集可解检查。

用户只看到一个 “Auto” 后端；内部是 HiGHS 快路径 + Rust 保险丝。

### 方案 B：纯 Rust

保留精确内核，借 HiGHS 的算法思想实现：

- presolve + 一致缩放；
- dual simplex + Harris + bound flipping；
- 稀疏 LU + Forrest–Tomlin；
- 精确重构/证书/Farkas；
- 删除 route/anytime/relevance 主路径。

这是长期工程；短期不应继续在现有 Rust 搜索层上加启发式。

### 不建议

- 把 Clarabel 当自动规划主后端（无 crossover、稠密）。
- 把 PDLP 当顶点写回后端（一阶法、通常无标准基输出；Gurobi 的 PDHG 已有 crossover，不能一概而论）。
- 把 microlp 当百万列主求解器（规模/数值不够）。
- 只缩放 RHS 而不检查写回（旧 `rhs_scale` 的教训）。

## 6. 实施优先级

| 优先级 | 工作 | 理由 |
|---|---|---|
| P0 | 模型一致缩放/精确输入 + 写回子集检查 | 解决 1/s vs 10000/s、小系数被当零、95 变量写回不可行 |
| P0 | HiGHS 状态/错误原样透出；Clarabel 状态检查 | 避免 AlmostSolved 被当最优 |
| P1 | HiGHS basis → Rust 精确重构/认证 | 速度 + 精度 |
| P1 | 保留 exact Farkas / support repair | 不可行与算不动的区分 |
| P2 | Forrest–Tomlin / 稀疏 LU / hyper-sparsity | 如果走纯 Rust |
| P2 | 列生成/惰性列提供器 | 如果枚举列成为瓶颈 |
| P3 | IPM + crossover 对照 | 仅作为下界/验证，不做主路径 |

## 7. 参考来源

- HiGHS options：<https://ergo-code.github.io/HiGHS/dev/options/definitions/>
- HiGHS 1.14 manpage（solver / run_crossover）：<https://manpages.ubuntu.com/manpages/stonking/man1/highs.1.html>
- Huangfu & Hall, *Parallelizing the dual revised simplex method*：<https://doi.org/10.1007/s12532-017-0130-5>
- OR-Tools, *Advanced LP Solving*（算法家族、顶点、crossover、容差）：<https://developers.google.com/optimization/lp/lp_advanced>
- Applegate et al., *Practical Large-Scale LP using PDLP*：<https://arxiv.org/abs/2106.04756>
- SoPlex exact mode：<https://soplex.zib.de/doc-7.1.4/html/EXACT.php>
- QSopt_ex rational LP：<https://www.math.uwaterloo.ca/~bico/qsopt/ex/>
- Gurobi, barrier/crossover：<https://support.gurobi.com/hc/en-us/articles/360013196832-Does-the-barrier-algorithm-return-a-basic-solution-for-LPs>
- Forrest & Tomlin, updated triangular factors：<https://dblp.org/rec/journals/mp/ForrestT72.html>
- Harris, Devex pivot selection：<https://dl.acm.org/doi/10.1007/BF01580108>
- Dual simplex progress / bound flipping：<https://dlnext.acm.org/doi/10.1007/s10589-008-9207-4>
- Kelner & Spielman, randomized simplex：<https://dl.acm.org/doi/10.1145/1132516.1132524>
- Iterative refinement for LP：<https://optimization-online.org/2015/06/4948/>
- Clp：<https://github.com/coin-or/Clp>
- GLPK：<https://www.gnu.org/software/glpk/>
- HiGHS 作者论文（simplex 工程/Harris/BFRT）：<https://arxiv.org/html/1503.01889>
- SoPlex 功能：<https://soplex.zib.de/>
- Gurobi PDHG / sifting / crossover 参数：<https://docs.gurobi.com/projects/optimizer/en/current/reference/parameters.html>
- CPLEX sifting：<https://www.ibm.com/docs/en/icos/22.1.1?topic=problem-sifting-optimizer>
- CPLEX Farkas：<https://www.ibm.com/docs/en/cofz/12.9.0?topic=cpxxdualfarkas-cpxdualfarkas>
- PDLP 数学（corrected dual objective）：<https://developers.google.com/optimization/lp/pdlp_math>
- QSopt_ex 论文：<https://doi.org/10.1016/j.orl.2006.12.010>
- SoPlex precision boosting：<https://arxiv.org/abs/2311.08037>
- soplex-rs：<https://github.com/scipopt/soplex-rs>
- soplex-sys（本仓库 vendor patch 的基线）：<https://github.com/mmghannam/soplex-sys>
- highs-sys（成熟 -sys 构建参考）：<https://github.com/rust-or/highs-sys>
