# 普通工厂与宇宙工厂的一致性链路

普通工厂继续以 `DualVar` 表达流，宇宙工厂以 `LocatedDualVar` 表达流。
两者共用机制展开、转换流算法、LP 后端和 `SolveJobs` 作业实现；宇宙额外注入运输关系。
不通过伪造普通工厂文档来运行宇宙求解。

## 求解与自动规划

- `UniverseSnapshot` 包含项目快照和真实宇宙文档。取快照持短锁，可达性、枚举和求解在锁外。
- 普通与宇宙作业使用同一个泛型 `SolveJobs`，分别存储槽位，不会因两类 ID 数值相同互相命中。
  两者都有缓存、排队超时、取消、输入失效检查和 latest-wins 重试；持续编辑超过重试轮数时
  返回重试错误，不返回已知过期的结果。
- `plan_universe_with_progress` 是只读计算；回写前校验目标宇宙、项目设置、规划偏好、
  可达性代次和原型仓库身份。其他工厂的普通编辑不会造成错误回写冲突。
- `apply_universe_auto_plan` 通过 commit 追加历史、revision、dirty 与持久化命令。
  写回严格供给，保留可复用 ID；等价计划不产生一次新的编辑。
- 保留模式保留停用条目，同时允许为实际需要的同配置追加启用条目。普通工厂采用同一规则。
- 宇宙火箭发运与轨道投送枚举允许品质域内的各品质；回写保留所有非零支撑，不用绝对阈值丢弃微量机制。
- 两类问题在添加转换流前都收集目标、表达式项和外部来源。`conversion_targets` 共用流键收集，
  避免只有表达式引用的温度区间或燃料类别缺少转换。

## 副作用与诊断

- 宇宙动作通过 `RecomputeUniverse` / `AutoPlanUniverse` / `CleanupUniverse` 执行后端副作用。
  GUI、MCP 与无界面命令不依赖选中页面的定时器才获得计算结果。
- 兼容机器选择与插件槽裁剪共用同一套 scoped 实现。兼容性自动选机后也裁剪插件。
  未完成配方卡与普通工厂一样作为草稿，不使整个宇宙求解直接报错。
- 地点、目标表达式流引用和品质上限检查覆盖两种工作区的目标、表达式、外部输入和机制。
- 宇宙清理先求解并校验快照，再一次性提交，撤销一次即可恢复整次清理。
- 规划失败保留原始求解原因以及带地点的无供给/无消耗对象。不将“不支持后端”或数值失败
  改写成“候选无可行解”。前端诊断、搜索和复制保留这些对象。
- 宇宙结果复用完整 `SolveDiagnostics` 转换，保留相对误差、容差、证书、间隙与候选规划报告；
  `amount/rate/scale` 与普通工厂使用同一口径。规划候选与回写后问题的证据分别归属。
- `dispatch.universe_solve` 和 MCP 命令回执包含实际宇宙求解结果；GUI 同时接收有地点作用域的
  结果事件。`get_planning_state` 可用 `project + universe` 查询，且与 `factory` 互斥。

## 前端

- 后端派发负责编辑后的重算，删除原宇宙 180ms 补算定时器。
- 缓存按每个宇宙的计算输入失效。名称、排列顺序和时间显示单位不会清空结果，修改另一宇宙
  不会清空当前结果；修改成本、地点、目标和项目计算设置会失效。
- 撤销焦点可恢复宇宙工厂；目标与求解结果提供建议，采纳建议绑定打开时的工作区和地点。
- 宇宙外部输入支持键盘/鼠标排序；所选地点电热均已免费时，快捷按钮禁用。
- 自动规划进度使用带宇宙 ID 的作用域，复用阶段、变量/约束数量和结束通知。

## 回归验证

```powershell
cargo test -p metatorio-runtime --no-default-features --features microlp,rust-lp --lib --test universe_parity --test universe_workspace
cargo test -p metatorio-shell --no-default-features --features microlp,rust-lp --lib -- --skip mcp::auto_plan_assets_tests::
cd metatorio-app
node --test tests/*.test.mjs
pnpm check
pnpm test:universe-workspace-browser
pnpm test:layout
pnpm test:network-browser
pnpm test:external-inputs-browser
pnpm test:auto-plan-progress-browser
pnpm test:dual-var-browser
pnpm build
```

`universe_parity` 将审计的最小模型转成正向回归，包含精确后端下 `1e-12` 的必需机制。
shell 测试验证实际持久化、规划期间锁可用、机器修复、品质扫描与缓存重试。
浏览器测试使用真实 Svelte/Cytoscape 与模拟 IPC，不代替原生桌面或全量真实 dump 规划测试。

本次额外抽跑真实数据 `base_accumulator` 时，microlp 在回写后的重算中返回
`Singular matrix`。在修改前 `91fc9ec` 的独立源码快照和独立 target 目录中复跑同一命令，
也出现同样失败。这是已有后端数值问题，本次未修改 LP，也不将该用例记为通过。
