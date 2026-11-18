# 宇宙工厂（Universe Factory）设计与数学模型

普通工厂仅规划单一的隐式环境（单一星球或地表）。**宇宙工厂（Universe Factory）** 支持将多个空间位置（行星、太空轨道）纳入统一的数学规划模型中，专用于求解《异星工厂：太空时代》（Factorio: Space Age）及大型跨星球 Mod（如 Space Exploration）中的多星球复合供应链。

---

## 1. 数据模型与代数抽象

### 1.1 空间位置抽象（`Location`）
- `Location` 定义为：
  - `Planet(name)`：具体行星地表（名称同时对应游戏中的 Surface 名；当前游戏数据不支持同星球多分地表）；
  - `Space`：太空轨道环境。
- **空间流坐标（`LocatedDualVar`）**：
  $$\text{LocatedDualVar} = (\text{Location}, \text{Flow})$$
  表示在特定空间位置上的单一物质、能量或运力流。这是全局线性规划（LP）对偶变量的基本坐标单元。

### 1.2 机制包装与星际物流机制（`LocatedMechanic`）
- **本地机制（`LocatedMechanic::Local`）**：
  包装普通的生产机制（`Mechanic`）并赋予空间位置 $(\text{Location}, \text{Mechanic})$。展开时严格采用该位置对应的环境属性（如行星太阳能倍率、引力、磁场及地表流体可用性）。
- **火箭发射出口（`LocatedMechanic::RocketExport`）**：
  - **逻辑**：在行星地表消耗 1 单位物品及其基于原型推导的物理重量（$\text{weight}$），同时消耗发射地由火箭发射井配方产出的 $\text{RocketWeightCapacity}$（火箭运力）；
  - **产物**：在 `Space`（太空）中产出 1 单位对应品质的相同物品；
  - **估算成本**：默认按物品实际质量（单位：千克）计入目标函数。
- **轨道空投降落（`LocatedMechanic::OrbitalDrop`）**：
  - **逻辑**：在 `Space` 消耗 1 单位物品，直接零物理运力降落到目标行星地表；
  - **边际成本模型**：
    每单位物品的转移成本包含堆叠成本与变质惩罚项：
    $$\text{Cost} = \frac{64}{\text{stack\_size}} + \frac{C}{\text{effective\_spoil\_ticks}}$$
    （非变质易腐物品略去第二项变质惩罚；两方向物流均严格继承物品品质）。
  - **边界约束**：`RocketExport` 与 `OrbitalDrop` 的源与目的位置均不允许为 `Space` 自环。

---

## 2. 物料平衡与转换边界

1. **局部流守恒**：
   每个 `Location` 拥有独立的物质/能量流守恒方程 $\sum \text{Output} - \sum \text{Input} = 0$。
2. **环境隐式源**：
   行星地表的无限天然资源（如矿脉、水体）按行星原型生成；太空（`Space`）的隐式原料仅限于行星轨道中的小行星碎块（Asteroid Chunks）。
3. **跨星系流通唯一通道**：
   太空物品仅能通过显式的 `OrbitalDrop` 降落至目标星球；地表物品进入太空必须消耗本地火箭发射井生成的 `RocketWeightCapacity`。

---

## 3. UI 工作区与交互设计

- **统一入口**：项目顶部导航栏中，普通工厂右侧展示宇宙工厂列表，点击 **「✦」** 即可创建。
- **独立环境所有权**：宇宙工厂工作区无需统一指定环境与严格约束，各本地机制直接绑定并继承所在 `Location` 的物理环境。
- **混合目标与表达式**：
  - 生产目标中的任意线性表达式（`TargetExpression`）支持每个项独立指定 `Location`；
  - 用户可在单个数学表达式中混合不同星球或太空的产物需求（例如：$\text{Space.PromethiumScience} \ge 10 \land \text{Nauvis.IronPlate} \le 500$）。
- **联合自动规划（Joint Auto-Planning）**：
  - 自动规划器会同时枚举所有涉及行星的本地机制候选、太空小行星捕获候选，以及跨星球各品质阶梯的火箭出口与轨道空投物流，最后组装为一个联合大型稀疏 LP 模型统一求解。
- **层级展示与布局**：
  - 求解结果与机制流网络支持按 `Location` 进行聚类展示；
  - 聚类分组仅影响界面视图与 ELK 网络图层渲染，绝不篡改工程文档顺序或线性规划输入矩阵。

---

## 4. 传输成本参数与配置

物流成本估算参数存储于 `ProjectSettings.orbital_transport_costs` 中：
- `rocket_export_per_kg`：每千克火箭运力基准成本（默认：$64^3 = 262144$）；
- `orbital_drop_per_stack`：轨道空投每堆叠基准成本（默认：$64$）；
- `orbital_drop_spoil_penalty`：变质惩罚系数（默认：$64^3 = 262144$）。

有效变质刻（$\text{effective\_spoil\_ticks}$）严格代入品质体系的变质倍率修正。用户可在项目设置侧边栏中随时调整以上参数，修改即时触发快照重算与撤销栈记录。

---

## 5. 回归验证与自动化测试

核心业务逻辑在 `tests/orbital_cargo.rs` 中完整覆盖：
- 火箭运载方向、品质继承、运力重量核算；
- 堆叠损耗、缺失或停用空投时的不可行诊断；
- 联合规划回写与可撤销编辑事务。

运行测试命令：
```powershell
cargo test -p metatorio-runtime --no-default-features --features microlp,rust-lp,soplex --test orbital_cargo
```
