# 聚变机制与邻居数估计

项目设置 `reactor_neighbours` 是建筑原型名到邻居数的映射，核反应堆和聚变反应堆共用。
没有覆盖时使用 3，不根据端口数、反应堆种类或布局猜测。暂不提供项目设置编辑界面。
可通过 dispatch 写入整张映射（空映射清除覆盖），参与撤销、重做、保存和求解缓存失效：

```json
{"scope":"project","action":{"project":1,"action":{"set-reactor-neighbours":{"estimates":{"nuclear-reactor":3,"fusion-reactor":5}}}}}
```

机制 `neighbours: null`（或缺省）继承项目估计；显式数字优先，0 表示无邻居。
旧工程已写入的数字继续作为显式值读取。现有机制卡的邻居数输入框可清空以恢复继承。
暂不推导真实邻居布局、不根据反应堆数量扣除边界损失，也不模拟邻居负载的动态反馈。

## 流展开

- `fusion-reactor`：`reactor`（带品质）、`neighbours`、`fuel`。冷却液和等离子体等量转换，
  以无邻居时输出流体的能量核算燃料。品质缩放流量与燃料，邻居只改变输出温度；
  `power_input` 和电源 drain 单独核算，不跟随品质缩放。显式燃料必须与 burner 类别匹配。
- `fusion-generator`：`generator`（带品质）、`temperature`。输入输出流体读取原型过滤器。
  热模式使用 `T * heat_capacity * effectivity`，而非普通发电机的温差公式；
  燃料模式使用 `fuel_value * effectivity`。实际流量同时受 `max_fluid_usage` 和
  `output_flow_limit` 约束，返回等量输出流体，温度为输出流体默认值。品质缩放流量/功率上限。
- 实体成本和寒冷星球供暖复用已有规则。宇宙工厂按所在地分别守恒所有流。

原型温度表收录聚变堆 `target_temperature`（缺省取输出流体默认温度）经邻居加成后的整数档位，
以及发电机返回流体默认温度。端口数量仅用于扩大温度词汇，不用于选择邻居数。
枚举阶段另合并项目覆盖的温度，原型缓存不因项目设置改变而污染。
输出温度与温度打表共用计算函数，沿用现有温度身份的向零截断策略。

普通/宇宙工厂自动规划均枚举两类新机制，并复用现有品质、可达性和表面限制过滤。
纯温度发电机按可用温度展开候选；燃料型仅需一条覆盖其输入温度范围的候选。
模型是满负载稳态估计，不验证黑启动电源、真实管路连通、布局或持续动态负载。

## 回归

- `cargo test -p metatorio-core --test fusion`：真实 assets 与小型原型，温度、品质、燃料、返液、供暖和成本。
- `cargo test -p metatorio-runtime --no-default-features --features microlp,rust-lp --test fusion`：项目设置、历史/缓存、枚举、普通和宇宙规划回写。
- `cargo test -p metatorio-shell --no-default-features --features microlp,rust-lp --lib fusion_tests`：目录、悬停、品质和建议。
- `pnpm test:fusion-browser`：共享编辑器、继承/手动邻居、燃料选择、温度和网络品质图标。
