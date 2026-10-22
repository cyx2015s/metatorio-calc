# LP 后端系统审查（2026-10-22）

审查基线：`5ce46b038f4ebbeb24f0f739e0510f1503d87f48`。启动时工作区干净。本轮不修改生产求解行为；新增诊断探针和此报告。结论来自当前源码和下列小规模验证，历史矩阵结果不作为当前版本的全量验收。

## 当前实际使用的后端

- shell、runtime、solver 默认开启 `rust-lp`；`lp.rs::select_backend` 在未设置环境变量时选择 `rust-certified`。`highs` 是可选 feature，`lp-experiments` 同时启用两者。不能再按旧文档把 HiGHS 当作默认后端。
- 主调用链：`plan_auto_plan_with_progress` → `enumerate_all` → `build_autoplan_problem` → `SolverProblem` 求解 → `solve_lp_named` → `Snapshot` → `lp_hybrid::solve_rust_certified` → `metatorio-lp`。
- Rust 路径：零传播不可行证明预检；浮点候选（首轮相关性缩减，后续不同容差／零传播）；逐行相对验证或精确认证／修复；必要时完整定价列生成、约束生成，最后返回原模型空间。
- `rust-certified` 默认允许 `1e-12` 相对物料平衡预算；`METATORIO_LP_BALANCE_MODE=exact` 改变这一验收路径。名称中的 certified 不意味着每次返回的浮点向量都是精确最优点。`SolveReport` 已分开记录 exact primal、exact optimality、relative balance 和 relative duality gap；应保留这些区别。
- `rust-exact` 为纯有理数冷启动；`rust-verified` 使用浮点搜索后精确修复；`hybrid-exact` 使用可选 HiGHS 候选。普通 `highs` 仅有浮点原问题检查，不能和 Rust 认证模式按相同质量保证比较。
- 回写还有选中机制子集的重新建模和自检，之后文档重算亦可能再解。一次用户规划与一次 LP 调用不是同一计时单位。

## 优化优先级

### 1. 复用同一候选已经生成的完整证书（低风险，先做）

`metatorio-lp/src/certified.rs:115` 和 `:279` 调用 `scale_to_dual_feasible`，该函数在 `lib.rs:851` 的内部接口已经生成 adjusted solution 的完整证书，但公开包装丢弃证书；随后 `certified_result`（`:417`）再认证同一个 adjusted solution。

`columns.rs` 已接入 `scale_to_dual_feasible_certified` 以消除同类重复，主认证入口仍未复用。建议在内核内部传递 `(solution, certificate)`，约束证书必须属于同一个完整模型、同一组值和对偶。禁止拿缩放前或子模型证书替代缩放后原模型认证。没有必要通过放宽容差换取这部分收益。

精确 primal simplex 还存在 `certify` 后紧接 `reduced_costs` 的重复全局定价机会。可把完整 pricing 结果作为单轮局部状态复用；不要设计跨不同 basis 或模型的隐式缓存。

### 2. 把取消、截止时间和运行身份贯穿 shell → runtime → solver（高收益的资源控制）

`solve_jobs.rs:164–180` 的 `spawn_blocking` 在外层等待超时后继续运行；返回时同键锁释放，后台结果丢弃。随后重试会启动新计算，旧计算并未停止。这是代码明确实现的行为，不是本轮观察到的真实用户负载统计。

内核已有 `SolveControl::with_cancellation`，但全仓调用仅在内核测试中。生产适配只创建局部 deadline，未把 shell 的取消信号传下去。普通重算等待默认 120 秒，Rust 单次搜索预算为 1200 秒，外层等待与计算预算存在明显分离。

建议保持每个工厂的在途任务身份直到计算真正停止；超时设置取消标志，过时 revision 可主动取消；重试复用仍运行的同一任务或在其退出后启动。不同工厂还应考虑全局计算/内存额度，避免多个稠密 LP 同时争抢内存。取消仍须返回未完成，不能变成不可行。

### 3. 稀疏基分解／更新：最大结构性瓶颈，不能只抬行数上限

浮点矩阵输入是稀疏的，但 `float.rs:368` 分配 `m*m` 的基逆，初始 basic values 运算以及 pivot 外积仍走稠密路径；重分解也在 `:118` 还原完整基逆。4096 行时仅一个 f64 逆矩阵为 128 MiB，尚不含模型、方向、临时矩阵和精确对象。

当前 `:261` 的 4096 行保护仍存在。它是这个浮点候选路径的资源保护，不是所有求解路径都拒绝大于 4096 行：约束生成、零传播等可能降低实际子问题维数。精确重建虽有分块分解，但 `lib.rs` 仍先分配结构基的稠密 Rational 矩阵；也不能把它当成完整稀疏实现。

建议分阶段引入稀疏 LU／基更新及稀疏三角求解，记录填充率、结构基维数、重分解次数、单次 pivot 和定价耗时。保留当前原模型认证作为交叉验收。单纯把 4096 改大或增加线程不会改变复杂度。

### 4. 减少适配层完整模型复制（中低风险，需先量化）

`lp.rs` 同时创建变量定义、变量索引、ParsedRow 和排序后的 Snapshot；`lp_hybrid.rs:124` 建预检 Model，`:340` 每次候选再建 Model，恢复路径还会转换。`Snapshot::exact_model` 实际生成 f64 稀疏 Model，不是把全模型立即转成 BigRational，但仍复制全部列和矩阵项。

建议每次 LP 请求只构造一次不可变 Model，预检、候选和恢复都借用；缓存所需稀疏索引，基与子模型状态单独持有。使用已规范化的数值模型作为主接口，Snapshot 作为按需序列化格式。先测构造时间和峰值内存，再推进接口改造；不能把所有求解耗时归因于这些复制。

### 5. 保留候选基／恢复状态，减少失败后的冷启动（中等风险）

`SearchWork` 保存第一轮 values 和 incumbent，但没有跨 attempt 的 basis／factorization。候选重试重新建模搜索，列生成恢复主要由第一轮通过验证的 values 决定是否有种子；后续更有价值的候选不能自动成为这一路径的种子。

建议记录每次候选的可行性、支持集、basis 和验证诊断；在相同列/行身份及边界下选最佳可用种子。数值多样性重试仍可保留冷启动，但不应无条件丢失已有进展。以精良板材等历史困难模型做限时对照，分别统计 Phase I、修复、定价与最终认证。

### 6. 真正惰性枚举应移到 runtime，而非再裁剪现成 LP（高收益、高复杂度）

目前先全量枚举机制，再展开所有 LP 列；内核列生成接受的仍是完整 Model。它能减少受限主问题求解规模，不能节省前端枚举和完整模型驻留。

已有 `lp-two-stage-column-generation-20261022.md` 及其反例测试正确指出：只对第一阶段活跃配方展开混合插件，既可能漏最优，也可能误判无解。建议后续做机制族列提供器，并保留全局 reduced-cost 和 Farkas 定价。不能把“只保留活跃路线”作为无损优化。

## 可选 HiGHS 的单独正确性问题

`build_problem` 放大 RHS，却保留原变量 lower/upper；返回时所有变量又除以 rhs_scale。因此有限边界／非零下界没有按变量代换同步变换。返回的 `evaluate_solution` 只检查约束行，不检查变量边界。

当前业务 `solver.rs:374/415/426` 创建的变量均为零下界、无穷上界，因此这个问题不直接解释当前默认工厂慢例。默认 Rust 路径使用未缩放 Snapshot 并提前返回，也不受这段缩放影响。但公共 LP 适配接受一般边界，切换到 HiGHS 时应修正边界缩放并用原始边界验收；不能以当前生产变量恰好特殊为理由保留缺陷。

另外，HiGHS 重试只覆盖求解调用返回的 Other/Str；求解返回后未通过原问题验证不会进入重试。适配没有设置统一时间预算。若保留其作为对比或候选后端，建议将验证失败也纳入有界重试，并在每次策略结果中记录状态与残差。未经认证的 HiGHS Infeasible 与精确 Farkas 证明应继续区分。

本轮 HiGHS 动态探针未完成：首次 CMake 自动选择不支持的 `Visual Studio 18 2026` generator；改用仓库脚本指定的 Ninja 后，编译器试编译长时间停滞，已终止本轮启动的构建进程。以上 HiGHS 缺陷是静态代码及变量代换推导，不能记成当前二进制实测复现。

## 新验证到的接口能力边界

将同一组边界探针运行在当前默认 `rust-certified` 后，零下界／无限上界控制案例通过，但另外三个具有显然可行最优点的模型均被拒绝：`min x, x>=1`（变量下界）；再加独立 `y=1`；以及 `0<=x<=2, x=1`。共同错误为 `floating candidate requires nonnegative columns and costs, without upper bounds`。

这不是返回错误解，也不是证明模型不可行，而是默认搜索路径不支持一般变量边界。公共 `VariableDefinition` 暴露 min/max，且默认是自由变量，但默认浮点候选限制为零下界／无限上界／非负成本。现有生产变量满足该边界限制，不能以此推断当前工厂案例错误。

日志还显示同一个确定性的“不支持边界”被连续重试，并进入不支持该边界的列／行恢复路径。建议增加显式能力检查和结构化 `UnsupportedModel`，避免无意义重试；如要支持一般 LP，可先做保留身份的变量移位、上界转约束等标准化，并配套目标常数、证书映射和原边界回验。不要只改错误文本为 Infeasible。

## 验证与边界

- 本轮运行 `cargo test -p metatorio-solver --lib --tests --release`：46 个单元测试及 9 个集成测试通过，共 55，通过时未包含新建的可选 HiGHS ignored 探针。
- 新探针只作诊断，不改变生产实现；性能数据不设置脆弱的速度断言。
- 不运行长时间全矩阵；不把旧二进制回放或历史 641 案例报告称为当前验收；不承诺上述优化的端到端加速倍数。

### 本轮新增实测

1. `cargo test -p metatorio-lp --release --lib -- --test-threads=1`：87 passed、0 failed、2 ignored，执行 0.77 秒。加上上述 solver 测试，现有测试共 142 项通过；两个原有 ignored 微基准未运行。
2. `cargo run -p metatorio-lp --release --example audit_kernel_costs`：16,384 列／1 行，7 组、每组 10 次调用的中位数。certify 为 22.589 ms；scale（内部已认证）为 35.449 ms；scale 后再次 certify 为 58.561 ms。重复部分 23.112 ms，占最后一项约 39.5%，不是应用端到端加速率。
3. 同一探针对纯空约束行执行浮点搜索＋精确检查：256／1024／4096 行分别约 0.574／4.190／64.233 ms，均零 pivot；4097 行触发浮点 dense row limit。逆矩阵数组分别为 0.5／8／128 MiB，按分配公式计算，不是测得的进程 RSS。
4. `METATORIO_LP_BACKEND=rust-certified cargo test -p metatorio-solver --release --test highs_bounds_audit -- --ignored --nocapture`：1 passed、3 failed，失败即上一节记录的已知能力边界。本探针默认 ignored，用来暴露问题，不改变正常测试验收标准；没有把这些失败并入“142 项通过”。PowerShell 运行时需先设置 `$env:METATORIO_LP_BACKEND='rust-certified'`。
5. 新编译当前默认 feature 的 `lp_application_replay` 和 `lp_verify_witnesses`，重放以下历史冻结数值模型；每份新生成的见证再由独立验证器检查。所有结果 solved、精确最优见证有效、浮点舍入检查通过。

| 冻结模型 | 列／行 | 本轮适配求解耗时 | 最大舍入相对平衡误差 |
|---|---:|---:|---:|
| ammonia-baseline-existing 首个 LP | 20／37 | 0.0085 s | 3.1721e-17 |
| factory-small-1 历史 capture | 66／72 | 0.2521 s | 6.6443e-17 |
| factory-small-2 历史 capture | 66／72 | 0.2469 s | 6.6443e-17 |

后两个是相关的历史回写小模型，不应当成两类独立大工厂覆盖。此次回放不包含当前原型枚举、完整自动规划或 GUI 回写。时间含捕获，不含文件解析等前置 setup；只做一次，无前后性能比较。

输入来自 `local/lp-exp/ammonia-baseline-existing-20261015/` 和 `local/lp-exp/backend-comparison-isolated/factory-small-{1,2}/repeat-0/rust-certified/capture/`。旧 comparison manifest 指向的原始文件已不存在，故改用其保存的 capture；未运行旧二进制。新输出、原模型捕获和独立验证 JSON 保存在 `local/lp-backend-audit-20261022/`。

推荐实施顺序：先做同解证书复用、能力检查及跨层取消；再用稳定的阶段计时决定共享 Model 与 warm start 的收益；最后推进稀疏基和 runtime 惰性列提供器。各步均保持原模型验收，不能把更快返回拒绝算作性能成功。
