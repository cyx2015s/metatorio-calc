# LP 审查后的首批改进（2026-10-22）

基于 `5ce46b038f4ebbeb24f0f739e0510f1503d87f48` 和同日审查，实施三个范围明确的改进。没有改变精度预算、候选全集、目标函数或游戏数值。稀疏基分解、runtime 惰性列提供器、一般变量边界标准化及可选 HiGHS 边界缩放修复留在后续范围。

## 同解完整证书复用

`metatorio-lp/src/certified.rs` 的大基与小基入口现在复用 `scale_to_dual_feasible_certified` 返回的 adjusted solution 和完整证书。私有辅助函数在内部取得这对数据，调用方不能传入任意证书。后续组合或修改解的路径仍重新认证。小基路径同时去掉一次完整解克隆。

扩展精确预算边界测试覆盖 1 和 32 个 basic 变量：原候选对偶不可行，缩放确实修改对偶；独立认证检查返回解，精确 gap 边界保持一致，略收紧预算仍拒绝。历史微基准证明重复工作的存在，本轮不据此宣称端到端加速比例。

## 能力预检与错误分类

`rust-certified`／`rust-verified` 的各策略共同要求零下界、无限上界和非负成本。现在在预检和多策略搜索之前检查一次，失败返回 `ResolutionError::UnsupportedModel`；不再浪费多轮候选、列生成和约束生成重试。上层提示“不支持”，不会标为不可行或建议把相同模型原样重试。

检查使用聚合后的列成本，允许 Snapshot 中先正后负但总成本非负的重复目标项。无效输入仍走原有校验。没有新增变量数／行数能力拒绝，因为不同恢复路径可以处理不同规模。

## 超时／中止后的协作取消

新增 solver `CancellationToken`，在 shell 的 blocking worker 上安装线程局部同步作用域。runtime 和 solver 同步调用树读取该作用域；需要精确线性代数操作的地方将同一原子标志传入已有 `SolveControl`。嵌套作用域、正常返回和 panic 都恢复原值；另起线程不会自动继承，需显式安装克隆的 token。

shell 超时或调用方丢弃 future 时发出取消。每个工厂的 owned key guard 随 worker 移动，直到 worker 真正退出才释放；因此旧任务未退出时重试不会再启动同键计算。锁等待本身也有超时，取消结果不进入缓存。正常的同键复用、跨工厂并行和文档变更重算逻辑保留。

runtime 在枚举、求解、回写验证阶段边界检查；Rust 候选、重试、恢复、精确操作和 LP 返回处也检查。取消表示“计算未完成”。可选 HiGHS 只在原生调用之间检查，不能强制中断已经运行的 native 调用；枚举／展开、部分整模型精确认证、单个大整数运算仍存在非抢占区。这不是硬实时取消，但这些阶段未退出时不会叠加同键计算。

## 验证记录

- `cargo test --release -p metatorio-lp -p metatorio-solver --lib --tests`：内核 87 项通过、2 个原有微基准 ignored；当时 solver 50 项单元测试和 9 项集成测试通过，4 个审查用边界探针 ignored。之后补齐冷启动取消及入口回归，最终 solver 结果另记于下。
- `cargo test --release -p metatorio-shell --lib solve_jobs::tests -- --test-threads=1`：9 项通过，覆盖超时／abort、同键不重叠、缓存复用、跨工厂并行及文档更新。
- `cargo test --release -p metatorio-runtime --lib cancelled_snapshot_and_auto_plan_remain_unfinished`：1 项通过。
- 默认能力限制的旧 ignored 探针仍用于审查一般边界，不把“支持一般 LP”当成本次已经完成的能力；新增正式测试验证返回 UnsupportedModel、聚合目标成本及无效输入分类。

- 最终源码 `cargo test --release -p metatorio-solver --lib --tests`：51 项单元测试、9 项集成测试通过，4 个审查探针 ignored。包含三个 Rust 入口的取消传播／恢复、聚合目标溢出等检查。以上去重后共 157 项通过；未运行完整 shell／runtime 测试集或长矩阵。
- 重新构建 `lp_application_replay` 和 `lp_verify_witnesses`，重放上轮保存的三个原模型：20 列／37 行的 ammonia-existing，以及两份相关的 66 列／72 行 factory-small 回写模型。三份均 solved、独立精确最优见证及浮点舍入检查通过，目标值和全部返回值的 f64 位模式与修改前一致。
- 回放约为 0.0045／0.2383／0.2542 秒，仅是单次烟测，不据此计算加速比。证据（输入哈希、二进制哈希、前后比较、见证、独立校验）保存于 `local/lp-backend-improvements-20261022/`。
- `git diff --check` 与变更 Rust 文件的 `rustfmt --check` 通过。可选 HiGHS 原生构建未在本轮重新验证。
