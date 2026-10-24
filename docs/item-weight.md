# 物品重量

依据 [Factorio 2.1.23 Item Weight](https://lua-api.factorio.com/2.1.23/auxiliary/item-weight.html)。

## 数据与计算

`ItemComponent.weight` 仍表示 dump 中的显式值，不写入推导结果。
`PrototypeStore::item_weight(name)` 返回缓存的 `ItemWeight`：`weight`（克）、
`source`（explicit / cursor / recipe / default）、选用的 `recipe` 及默认值回退原因。
缓存属于游戏上下文的原型仓库，不依赖工厂、解锁状态、插件或物品品质。
`WeightConstants` 从 dump 的 `utility-constants` 读取默认物品重量与默认火箭运力；
小型测试 dump 缺少这两个字段时使用原版的 100 g / 1,000,000 g。
这些常数不再放在可变的 `GameState` 中。

- 显式重量优先；否则同时含 only-in-cursor 和 spawnable 标记的物品重量为零。
- 过滤 hidden 和不允许 decomposition 的配方，再依次比较同名、非催化剂、
  allow_as_intermediate、类别、子组顺序、配方顺序。平局使用名字保证稳定。
  子组与顺序缺省时复用现有的主产物派生逻辑。
- 递归累加物品原料重量，流体原料按 100 g/单位；所有物品产物按概率期望计数，
  流体产物不计数。乘物品的 ingredient_to_weight_coefficient 后进行整组载荷调整。
- 循环依赖在重复访问处临时使用默认重量，再完成外层配方计算；遍历物品按名字排序。
  使用显式栈，避免长模组依赖链耗尽 Rust 调用栈。
- 显式重量和最终整组除法保留 1/65536 g 定点精度；配方中间重量截为整数克。

整数中间值、循环和子组顺序在本机 **2.1.22** 的隔离模组实验中核实；
它们是对文档未明确描述细节的补充验证，不等于已在 2.1.23 客户端运行过。
实验还发现 2.1.22 对小于 1 g、被截到零的配方中间重量可能产生极大重量。
这里对该退化情况返回零，避免复刻溢出行为；该异常样本不计入游戏一致性对比。

## 展开与界面

重量发射机制读取同一缓存，以火箭仓实际 lift_weight 计算整件装载数，
不再用缺省 100 g 代替自动重量，也不允许装入半件或超重物品。
火箭仓未给出运力时，容量产生与消费统一使用仓库中的默认运力。
原有槽位发射模式保持原逻辑；零重量不生成重量发射变量。

`calculation.item_weight` 透传来源；`calculation.stats` 增加 grams 单位的
item-weight / stack-weight，以及使用默认火箭运力的 rocket-item-capacity。
悬停显示 g / kg / t、来源配方及回退原因，并注明品质不影响重量、实际火箭仓运力可能不同。

## 验证入口

- `cargo test -p metatorio-data --tests`：公式分支、概率、多产物、排序、循环、定点精度、
  3000 层依赖和两个 assets 数据集的全部物品。
- `cargo test -p metatorio-core --test mechanic_flows --no-default-features`：整件发射、品质无关与既有机制回归。
- `cargo test -p metatorio-shell --lib prototype_calculation::tests --no-default-features`：悬停数据与缓存一致。
- `pnpm test:item-weight` / `pnpm test:item-weight-browser`：单位、品质选择、真实 Svelte 悬停排版。

需要游戏核对时，在隔离目录通过测试模组导出原始 dump 和 `prototypes.item[name].weight`，
设置 `METATORIO_WEIGHT_REFERENCE_DUMP` / `METATORIO_WEIGHT_REFERENCE_WEIGHTS` 后运行：

```powershell
cargo test -p metatorio-data --test item_weight game_export -- --ignored --nocapture
```

本次实验在 `local/item-weight-validation` 中，对比通过 350 个物品，包含原版与专用边界样本。
不修改安装目录、常用模组配置或用户存档。游戏相关测试默认忽略，常规测试不要求安装 Factorio。
