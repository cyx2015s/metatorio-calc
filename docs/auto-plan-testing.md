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

76 个测试入口，其中 73 个默认运行、3 个压力测试需要显式运行。部分入口包含多组参数，因此实际规划调用数大于入口数。

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
cargo test -p metatorio-shell --lib auto_plan_assets_tests::stress_scale_invariance -- --ignored --nocapture
```

两份上下文 × 铁板/电子电路/塑料 × `1e-9, 1e-6, 0.001, 1, 1000, 1e6, 1e9`，共 42 个规划案例。除一般断言外，还检查单位目标的成本在缩放前后相同，容差 `1e-4`。

### 插件、插件塔、品质组合

```powershell
cargo test -p metatorio-shell --lib auto_plan_assets_tests::stress_modules_beacons_quality_and_scale -- --ignored --nocapture
```

完整基础上下文 × 五种品质 × 三种目标量 × 有/无插件塔，共 30 组配置。每组启用最佳插件；插件塔配置包含 8 座、0.5 分摊和两个对应品质的三级速度插件。

可先限定前六组（普通品质、三种倍率、有/无插件塔）：

```powershell
$env:AUTOPLAN_STRESS_LIMIT = '6'
cargo test -p metatorio-shell --lib auto_plan_assets_tests::stress_modules_beacons_quality_and_scale -- --ignored --nocapture
Remove-Item Env:AUTOPLAN_STRESS_LIMIT
```

### 遍历所有配方物品产物

```powershell
cargo test -p metatorio-shell --lib auto_plan_assets_tests::stress_all_recipe_products -- --ignored --nocapture
```

默认遍历两份上下文中全部 1736 个不同物品产物（跨上下文重复名称分别测试）。每个目标使用完整上下文和新建项目，按名称排序，在默认 Nauvis 上以 `1/s` 运行；成功必须通过全部可行性断言。不能在当前星球生产的目标允许报告结构化失败，但错误不能为空。**因此该测试通过不意味着所有目标都可生产**，逐项结果必须一起检查。

分批运行，例如每份上下文取第 100 到第 129 个产物：

```powershell
$env:AUTOPLAN_SWEEP_OFFSET = '100'
$env:AUTOPLAN_SWEEP_LIMIT = '30'
cargo test -p metatorio-shell --lib auto_plan_assets_tests::stress_all_recipe_products -- --ignored --nocapture
Remove-Item Env:AUTOPLAN_SWEEP_OFFSET
Remove-Item Env:AUTOPLAN_SWEEP_LIMIT
```

结果逐条写入 `local/auto-plan-sweep-<offset>.jsonl`，包含上下文、产物、耗时、终态、错误及求解诊断。同一个 offset 再运行会覆盖该分片的报告。高复杂度测试可能耗时较长；每次异步规划的轮询上限为 180 秒，超时直接失败，不当作正常无解。

## 已复现的问题

`simultaneous_same_request_id_creates_only_one_project` 当前失败：两个并发请求使用相同 `request_id`，分别创建了不同的项目和工厂。原因是缓存查询发生在异步目录读取之前，而缓存写入发生在配置完成之后，中间没有为正在处理的幂等键占位或串行化。

这个测试保留真实的失败断言，未标记忽略。常规新增测试因此会返回失败；不能把“顺序重试通过”当作“并发幂等也通过”。本次只增加测试，没有修改生产实现。

## 本次运行记录（2026-10-12）

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
