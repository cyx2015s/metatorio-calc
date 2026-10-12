# 自动规划真实上下文测试

测试文件：`crates/metatorio-shell/src/mcp/auto_plan_assets_tests.rs`。

直接调用实际 MCP `auto_plan` 和 `get_planning_state` 处理器，走创建项目、配置、异步枚举、求解、版本检查、回写和结果分页全过程。没有另造简化规划器。测试不启动 GUI 或 HTTP 监听，不需要游戏安装目录，也不访问用户的应用数据目录；HTTP 传输、认证和协议握手不属于这批测试的覆盖范围。

上下文来自仓库中的两份完整数据，按 `CARGO_MANIFEST_DIR` 定位，缺文件直接失败，不静默跳过：

- `assets/data-raw-dump.json`：662 个配方，313 个不同的配方物品产物。
- `assets/data-raw-dump-heavily-modded.json`：4162 个配方，1423 个不同的配方物品产物。

两个上下文各解析一次并缓存。测试间串行枚举，避免默认并行测试让多个完整 LP 同时占用大量内存。各案例使用独立状态，只有专门的并发测试会共享状态。

## 常规回归

```powershell
cargo test -p metatorio-shell --lib auto_plan_assets_tests -- --test-threads=1
```

83 个测试入口，其中 79 个默认运行、4 个压力测试需要显式运行。部分入口包含多组参数，因此实际规划调用数大于入口数。

覆盖内容包括基础冶炼、三种电路、物流、化工、发动机、机器人、科研瓶、三级插件、五个星球、模组产业链、多目标、重复目标、目标顺序、物品/流体/电/热外部输入、温度点与区间、异步失败、非有限数值、配置失败回滚、失败后重试、顺序与并发幂等、独立项目并发、显式上下文绑定、重复规划不改版本、结果分页和外部供给成本单调性。

每个成功案例要求：

1. 异步状态从 `running` 进入 `done`，结果确实为 `Solved`。
2. 回写验证通过，约束违反量有限且不超过 `1e-5`。
3. 成本、机制用量、流量和缩放数值有限，机制用量非负。
4. 所有非虚拟机制 ID 都能在回写文档中找到。
5. 工厂启用严格供给，每个目标都有满足需求的产量。
6. 使用同步求解入口重新求解回写文档，检查可行性及成本一致性。
7. 不额外创建项目或工厂；配置名、上下文与外部输入数量正确。

三级插件与 Aquilo 案例显式声明缺少的进口物资；没有把严格供给关闭，也没有把目标本身作为外部输入。

## 压力测试

### 极端目标倍率

```powershell
cargo test --release -p metatorio-shell --lib auto_plan_assets_tests::stress_scale_invariance -- --ignored --nocapture
```

两份上下文 × 铁板/电子电路/塑料 × `1e-9, 1e-6, 0.001, 1, 1000, 1e6, 1e9`，共 42 个规划案例。除一般断言外，还检查单位目标的成本在缩放前后相同，容差 `1e-4`。

### 插件、插件塔、品质组合

```powershell
cargo test --release -p metatorio-shell --lib auto_plan_assets_tests::stress_modules_beacons_quality_and_scale -- --ignored --nocapture
```

完整基础上下文 × 五种品质 × 三种目标量 × 有/无插件塔，共 30 组配置。每组启用最佳插件；常见插件塔布局为 **8 座、share=8、总共 16 个对应品质的三级速度插件**。MCP 参数名为 `count`，模块 `count` 是这一组塔的插件总数，`share` 是每座塔平均覆盖的建筑数。此前的 share=0.5、总共两个插件的配置已纠正。

可先限定前六组（普通品质、三种倍率、有/无插件塔）：

```powershell
$env:AUTOPLAN_STRESS_LIMIT = '6'
cargo test --release -p metatorio-shell --lib auto_plan_assets_tests::stress_modules_beacons_quality_and_scale -- --ignored --nocapture
Remove-Item Env:AUTOPLAN_STRESS_LIMIT
```

### 按物品的显式进口地点规划

```powershell
cargo test --release -p metatorio-shell --lib auto_plan_assets_tests::stress_import_location_products -- --ignored --nocapture
```

范围改为原始 dump **显式设置** `default_import_location` 的物品，并排除指向 Aquilo 的物品。不会把 `ItemComponent` 自动填充的 Nauvis 默认值当作显式设置。兼顾 `item`、`capsule`、`ammo`、`gun` 等物品子类型，目前基础上下文 45 个、模组上下文 1 个，共 **46 个**。

每个目标使用自己的 `default_import_location` 作为 `planet`，普通机器品质、普通目标品质、不使用最佳插件、不设置外部进口，以 `1/s` 运行。成功必须通过全部可行性断言；失败逐条保存。进口地点本身不承诺整个产业链当地自给自足：两种 `overgrowth-*-soil` 配方都需要 Nauvis 的 `biter-egg`，它们在无进口的 Gleba 上作为明确的供给不足例外，仍实际调用规划并记录结果，不直接跳过。其余 44 个目标要求可解，出现任何新失败都会使测试失败。

分批运行，例如排序后的第 10 到第 19 个物品：

```powershell
$env:AUTOPLAN_SWEEP_OFFSET = '10'
$env:AUTOPLAN_SWEEP_LIMIT = '10'
cargo test --release -p metatorio-shell --lib auto_plan_assets_tests::stress_import_location_products -- --ignored --nocapture
Remove-Item Env:AUTOPLAN_SWEEP_OFFSET
Remove-Item Env:AUTOPLAN_SWEEP_LIMIT
```

结果逐条写入 `local/auto-plan-import-locations-<offset>.jsonl`，包含上下文、物品子类型、星球、完整请求配置、耗时、终态、错误、失败分类、成本及求解诊断。终态先落盘再断言，失败案例的记录也能留下。任何求解器内部错误（即便发生在预期缺少品质来源的目标上）都会使矩阵断言失败；超时等基础设施错误也不会被统计为正常无解。每次异步规划轮询上限为 180 秒。

### 主品质、目标品质、最佳插件与塔布局矩阵

```powershell
cargo test --release -p metatorio-shell --lib stress_import_location_quality_matrix -- --ignored --nocapture
```

完整矩阵为 46 个目标 × 5 种 `major_quality` × 5 种目标品质 × 最佳插件开/关 × 4 种塔布局，共 **9200 组**。插件品质跟随主品质，塔布局为：无塔；8/8/16（塔数/覆盖数/插件总数）；6/6/12 且塔内插件普通品质；12/8/24 且塔本体传奇品质。

`modules.best` 是 MCP 中对应“使用最佳插件”的参数；false 时不枚举机器自身插件，但可继续配置速度插件塔。最佳插件开启前后，在同一目标、主品质和塔布局下比较可行性与成本：关闭时可解，开启后不得无解或增加最优成本。没有匹配的关闭案例时（例如分片切开一对），不会伪造比较结果。

矩阵中的普通目标（除上面的虫卵进口例外）要求保持可解，不能将新出现的内部求解错误当作“合法目标不可达”放过。另外有普通回归用例验证：主品质设为传奇、没有品质插件或其他高品质来源时，三个星球科研瓶的精良和传奇目标必须报告真正无可行解，不能因机器品质高就凭空生成高品质产物。

较小的 `smoke` 矩阵为每个物品 20 组，涵盖全部五种主品质、全部五种目标品质、最佳插件开/关、无塔与标准 8/8/16 布局。可指定三个星球的科研瓶，运行 60 组：

```powershell
$env:AUTOPLAN_MATRIX_PROFILE = 'smoke'
$env:AUTOPLAN_ITEMS = 'metallurgic-science-pack,agricultural-science-pack,electromagnetic-science-pack'
cargo test --release -p metatorio-shell --lib stress_import_location_quality_matrix -- --ignored --nocapture
Remove-Item Env:AUTOPLAN_MATRIX_PROFILE
Remove-Item Env:AUTOPLAN_ITEMS
```

也可以设置 `AUTOPLAN_ASSET`（dump 文件名）、`AUTOPLAN_MATRIX_OFFSET`、`AUTOPLAN_MATRIX_LIMIT`。先过滤物品，再按配置、物品展开组合，最后按全局案例序号分片；这些控制不改变任何实际上下文、候选或求解设置。`AUTOPLAN_SWEEP_OFFSET/LIMIT` 则用于上面的普通目标遍历。

矩阵结果保存于 `local/auto-plan-quality-<full|smoke>-<offset>.jsonl`。相同报告名再次运行会覆盖。完整矩阵与高品质 LP 可能非常耗时，推荐发布模式并分片执行。

本机 VS 2026 配合较旧 CMake 时，HiGHS 的首次 release 编译会报不支持 `Visual Studio 18 2026` 生成器。可在已安装 Ninja 的环境中先设置 `$env:CMAKE_GENERATOR = 'Ninja'`，无需改动求解器或仓库构建配置。

## 已复现的问题

初次添加测试时，`simultaneous_same_request_id_creates_only_one_project` 失败：两个并发请求使用相同 `request_id`，分别创建了不同的项目和工厂。原因是缓存查询发生在异步目录读取之前，而缓存写入发生在配置完成之后，中间没有为正在处理的幂等键占位或串行化。仓库后续提交 `2e54799` 已包含该问题的修复。

这个测试仍保留真实断言，未标记忽略。下面的 72 通过、1 失败是最初运行的历史结果，不代表修复后的当前状态。本次缩小范围与扩展矩阵未修改生产实现。

## 初次运行记录（2026-10-12，缩小范围之前）

| 检查 | 实测结果 |
|---|---|
| 新增常规测试 | 72 通过，1 失败（上面的并发幂等问题），3 个压力入口默认忽略 |
| 原有 MCP 单元测试 | 12 通过 |
| 倍率压力测试 | 两份上下文的全部 42 组通过 |
| 最佳插件与插件塔压力测试 | 前 6 组普通品质组合通过；剩余 24 组尚未运行 |
| 全产物遍历首个分片 | 每份上下文前 30 个，共 60 个：31 个成功并通过断言，29 个返回结构化无解；其余 1676 个尚未运行 |

原始运行日志在 `local/auto-plan-assets-tests-final.log`、`local/auto-plan-existing-mcp-tests.log`、`local/auto-plan-scale-stress.log`、`local/auto-plan-module-stress.log`、`local/auto-plan-sweep-tests.log`。逐个产物的结果在 `local/auto-plan-sweep-0.jsonl`。`local/` 本来就在仓库忽略列表中，报告不会自动进入提交。

未放宽告警的 `cargo clippy -p metatorio-shell --tests --no-deps -- -D warnings` 被现有 `tests/allowed_modules_real.rs` 的 `collapsible_if` 与 `needless_borrow` 两项旧告警阻断；没有为此修改原有测试。

临时使用 `-A clippy::collapsible_if -A clippy::needless_borrow` 放宽这两条规则后，整个 shell 的测试静态检查通过；格式检查与差异空白检查也通过。

## 按进口地点修订后的发布模式结果（2026-10-12）

| 检查 | 实测结果 |
|---|---|
| 显式字段筛选与塔效果/耗电语义 | 两个测试通过；确认含胶囊/弹药/武器、不含隐式 Nauvis 或 Aquilo、8 塔对应 16 插件、share=8 的均摊耗电 |
| 全部 46 个对应星球的普通目标 | 44 可解并通过回写复算；2 个 Gleba 高级土壤因虫卵进口缺失无解 |
| 三种科研瓶的 smoke 品质矩阵 | 60 组：46 可解，12 个高品质无来源的预期无解，2 个求解器内部错误；矩阵断言失败 |
| 三种科研瓶的四种塔布局 | 24 组：20 可解，4 个求解器内部错误；矩阵断言失败 |
| 全部常规自动规划测试 | 77 通过，2 失败（下面两个新增回归），4 个压力入口忽略 |
| 无品质来源的高品质目标负例 | 三种科研瓶 × 精良/传奇目标，共 6 组通过；主品质均为传奇 |
| 严格 Clippy | 当前 checkout 不放宽告警也通过 `cargo clippy -p metatorio-shell --tests --no-deps -- -D warnings` |

普通目标 `1/s`、主品质普通、目标品质普通，已重复复现以下 **求解器内部错误**（错误原文明确说明不代表问题无可行解）：

- Fulgora 电磁科研瓶，8 塔 / share=8 / 总共 16 个普通速度插件3，最佳插件关闭。
- Gleba 农业科研瓶，相同 8/8/16 布局，最佳插件开启。
- Fulgora 电磁科研瓶，6 塔 / share=6 / 总共 12 个普通速度插件3，最佳插件关闭或开启，两种配置都失败。

前两种配置已分别保留为默认运行的 `electromagnetic_science_no_modules_standard_beacons_regression` 和 `agricultural_science_best_modules_standard_beacons_regression`。这些配置没有外部输入；无塔的普通目标可解，所以不能归因为选择了错误星球或缺少外星原料。新增测试没有改变求解器或自动规划的生产实现。

最终又运行了一个仅含两组的农业科研瓶分片（full 矩阵、只选农业瓶、offset=2、limit=2）：同样的普通品质 8/8/16 布局，关闭最佳插件可解，开启报内部错误。报告在 `local/auto-plan-quality-full-2.jsonl`，日志在 `local/auto-plan-quality-regression-recheck.log`。

报告：`local/auto-plan-import-locations-0.jsonl`、`local/auto-plan-quality-smoke-0.jsonl`、`local/auto-plan-quality-full-0.jsonl`。对应日志为 `local/auto-plan-import-location-release.log`、`local/auto-plan-quality-smoke-release.log`、`local/auto-plan-beacon-layouts-release.log`、`local/auto-plan-assets-current-release.log`。独立回归用例还写出 `local/auto-plan-regression-<科研瓶名>-0.jsonl`，包含失败请求的完整参数。

本次没有运行全部 9200 组矩阵；已执行 60 组品质矩阵和 24 组布局矩阵，部分配置重合。当前测试进程退出后再重新编译测试：Windows 会锁定正在运行的测试可执行文件。

## 求解器内部错误的修复（2026-10-12）

上面那两个默认运行的回归用例已修复，**修复在生产实现里**（crates/metatorio-solver）：

- **先补可诊断性**：good_lp 的 ResolutionError::Other 携带的原因（Unknown /
  NoSolutionFound / ModelError …）以前被丢掉，只剩一句「求解器内部错误」。
  现在原样写进错误串。实测：农业瓶是 NoSolutionFound，电磁瓶是 Unknown。
- **根因**：HiGHS 的单纯形在退化模型上会偶发**循环**（cycling），它宁可放弃求解
  也不冒死循环的风险，于是返回非终止状态——**不代表无可行解**。HiGHS 作者的结论
  就是「换个策略重跑」，见
  [ERGO-Code/HiGHS#1528](https://github.com/ERGO-Code/HiGHS/issues/1528)。
- **修法**：solve_with_retry 有界重试阶梯——默认选项 → 关均衡缩放
  （simplex_scale_strategy=0）→ 关缩放 + 关 presolve。只对**非终止**失败重试；
  Infeasible / Unbounded 是定论，不重试。重试换的只是数值路径，模型不变，
  解仍要过原问题校验（evaluate_solution，违反量不超过 ACCEPT_VIOLATION）才会被采用。

**验证**（发布模式，本机）：

| 检查 | 结果 |
|---|---|
| 两个回归用例，默认选项 | 稳定失败（可重复复现，本轮又复现了一次） |
| 两个回归用例，第二轮选项（关缩放） | 都通过 |
| 完整 auto_plan_assets_tests | **79 通过 / 0 失败 / 4 忽略** |

另外确认：presolve=off 单独只能修好农业瓶（电磁瓶仍失败），单纯改
simplex_strategy / simplex_price_strategy / simplex_dual_edge_weight_strategy 都无效
——所以重试阶梯第二轮用「关缩放」是实测选出来的，不是随手加的。
