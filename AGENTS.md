# AGENTS.md: 面向 AI Agent 的 Metatorio 架构与代码阅读指引

欢迎来到 **Metatorio（切向量化）** 仓库！本指南专门面向未来接手、分析或修改本项目的 AI Agent（以及人类核心贡献者）。

Metatorio 是一个针对《异星工厂》（Factorio 2.0+ 及 Space Age 太空时代 DLC、大型模组如 Py, SE, K2）的高性能运筹学量化、多星球物流调度与自动化工厂规划工具。

---

## 1. 架构总览与核心设计哲学

```
┌────────────────────────────────────────────────────────────────────────┐
│                        前端与命令行外壳层 (Shells)                     │
│   ┌──────────────────────────────────┐  ┌───────────────────────────┐  │
│   │ metatorio-app (Tauri v2+Svelte 5)│  │ metatorio-headless (MCP)  │  │
│   └──────────────────────────────────┘  └───────────────────────────┘  │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ 调用
┌───────────────────────────────────▼────────────────────────────────────┐
│                  metatorio-shell (外壳抽象、长任务、MCP)                │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ 调度状态与消息
┌───────────────────────────────────▼────────────────────────────────────┐
│          metatorio-runtime (应用层状态机、事务历史、联合求解、自动规划)         │
└─────────┬─────────────────────────┬─────────────────────────┬──────────┘
          │ 依赖                     │ 依赖                     │ 依赖
┌─────────▼────────┐      ┌─────────▼────────┐      ┌─────────▼──────────┐
│ metatorio-solver │      │  metatorio-core  │      │   metatorio-data   │
│ (LP建模与后端调度)│      │(机制物理展开/流) │      │(原型存储/容错解析) │
└─────────┬────────┘      └──────────────────┘      └─────────▲──────────┘
          │ 依赖                                              │
┌─────────▼────────┐                                ┌─────────┴──────────┐
│   metatorio-lp   │                                │metatorio-data-     │
│(有理数重构/基验证)│                                │codegen (代码生成器)│
└──────────────────┘                                └────────────────────┘
```

### 核心设计原则

1. **绝对的领域解耦与纯数据驱动**：
   `metatorio-core`、`metatorio-solver`、`metatorio-runtime` 没有任何 GUI 框架或 Webview 依赖。所有计算输入与输出均为纯数据结构，可以在命令行与 CI 环境独立测试。
2. **单向消息流与不可变事务历史**：
   所有状态变更都通过派发强类型消息 `AppMessage` 完成。`RuntimeState::dispatch` 是应用状态流转的单一入口，内置细粒度的 Undo/Redo 历史快照。
3. **有理数精确验证与数值稳定性**：
   针对复杂生产网络与极端模组可能产生的浮点病态矩阵，项目研发了 `metatorio-lp`，支持将输入视为完全精确的二进制有理数，并提供基解验证（Basis Verification）与不可行性证书推导。
4. **强类型零漂移 IPC 契约**：
   Rust 桌面端与前端 Svelte 之间通过 `tauri-specta` 自动生成强类型的 TypeScript 绑定（`bindings.ts`），严禁手写前后端接口胶水代码。

---

## 2. 代码仓库拓扑与 Crate 职责矩阵

| Crate 目录 | 角色定位 | 核心职责与关键结构 | 约束与禁止行为 |
| :--- | :--- | :--- | :--- |
| `crates/metatorio-core` | **领域核心层** | 物理机制展开（`expand`）、流标识（`DualVar`）、生产单元（`Mechanic`）、多星球位置（`Location`）、科技可达性（`Accessibility`）、产能（`productivity`）。 | **禁止** 引入任何求解器或 UI 依赖；保持纯代数与物理逻辑。 |
| `crates/metatorio-lp` | **纯数学内核** | 纯 Rust 连续 LP 精确有理数重构与基验证。提供 `Rational` (`BigRational`) 运算、对偶修复（`DualRepair`）、稀疏分解。 | **禁止** 引入特定游戏领域逻辑，保持通用运筹学计算。 |
| `crates/metatorio-solver` | **求解器调度** | 将工厂抽象为 LP 矩阵（`SolverData`），统一封装 HiGHS、SoPlex、Clarabel、microlp 与自研 LP 后端。 | 负责数学规划建模，输出 `SolverSolution` 与不可行证书。 |
| `crates/metatorio-data` | **原型数据仓储** | 存储并高效索引游戏原型（`PrototypeStore`）。处理 Lua dump 的宽松容错反序列化（`lenient`）、配方重量推导（`item_weight`）与聚变参数计算。 | 原型结构体由 `metatorio-data-codegen` 在 `build.rs` 编译期自动生成。 |
| `crates/metatorio-data-codegen` | **代码生成器** | 解析官方 `schema/prototype-api.json`，在编译期生成强类型的 Rust 原型组件。 | 仅在构建期由 `metatorio-data` 调用。 |
| `crates/metatorio-icons` | **原生图标渲染** | 无需启动 Factorio，在进程内直接读取 Mod zip 或本地目录图片，按游戏图层规则与品质像素级渲染合成 PNG。 | 内部使用直通 Alpha（Straight Alpha），与导出的预乘 Alpha 对齐。 |
| `crates/metatorio-runtime` | **应用状态机** | `AppDocument`、`ProjectDocument`、`FactoryDocument`、`UniverseDocument` 序列化；`AppMessage` 分发；自动规划（`auto_plan`）；多星球联合求解（`universe`）。 | **核心业务枢纽**。不碰 GUI，仅产出 `CommandEffect` 与 `Diagnostic`。 |
| `crates/metatorio-shell` | **宿主服务层** | 抽象 `Host` trait（系统事件广播、数据目录）；内置长耗时求解防抖排队（`solve_jobs`）；内置 Model Context Protocol（`mcp`）服务端。 | 支撑 GUI 与 Headless 共享同一套业务状态 `AppState`。 |
| `crates/metatorio-headless` | **无头 MCP 进程** | 独立的命令行可执行文件，不链接 WebKit/GTK，专用于服务器与 AI Agent 接入。 | 共享相同 AppData 目录与 MCP 工具集。 |
| `metatorio-app/src-tauri` | **桌面外壳** | Tauri 2.0 适配器。提供 `TauriHost`、窗口控制、单实例通信与 `specta` IPC 导出。 | 仅做参数转交，所有业务委托给 `metatorio-shell`。 |
| `metatorio-app/src` | **桌面 UI 前端** | Svelte 5 (Runes 响应式) 现代化前端。渲染机制卡片、ELK 网络拓扑图、流平衡、表达式编辑器。 | 前端包管理使用 `pnpm`；严禁手动修改 `bindings.ts`。 |

---

## 3. 核心领域模型与关键概念速查

### 3.1 生产单元：`Mechanic`
定义在 `crates/metatorio-core/src/mechanic.rs`，代表工厂中 1 个具体的生产单元（不是工厂整体）：
- `Recipe(RecipeMechanic)` / `Mining(MiningMechanic)` / `Plant(PlantMechanic)` / `Spoil(SpoilMechanic)`：配方、采矿、种植与变质；
- `ItemFuel(ItemFuelMechanic)` / `FluidFuel(FluidFuelMechanic)` / `FluidHeat(FluidHeatMechanic)` / `ItemLaunch(ItemLaunchMechanic)`：燃料转换、流体热量与火箭单件发射；
- `Generator(GeneratorMechanic)` / `Boiler(BoilerMechanic)` / `Reactor(ReactorMechanic)` / `FusionReactor(FusionReactorMechanic)` / `FusionGenerator(FusionGeneratorMechanic)` / `Solar(SolarMechanic)`：各类发电与供热装置；
- `TileExtract(TileExtractMechanic)` / `TileDispose(TileDisposeMechanic)`：地表流体抽取与物品倾倒；
- `VirtualScience(VirtualScienceMechanic)`：虚拟科研容量折算。

### 3.2 对偶变量与流平衡：`DualVar`
定义在 `crates/metatorio-core/src/dual_var.rs`，对应 LP 模型中的每条守恒约束行：
- `Item(IdWithQuality)` / `Entity(IdWithQuality)` / `Fluid { name, temperature }`：标准物品、实体与带温度区间流体；
- `Electricity` / `Heat` / `FluidHeat { filter }` / `FluidFuel { filter }` / `ItemFuel` / `ItemFuelSupply` / `Pollution { name }`：能源、燃料与污染；
- `RocketWeightCapacity` / `RocketSlotCapacity` / `VirtualScience(String)`：火箭运力与虚拟科研；
- `Custom { name }`：用户自定义流（用于任意线性表达式）；
- 跨星球坐标包装：`LocatedDualVar { location, flow }`，其中 `Location` 为 `Planet(name)` 或 `Space`。

### 3.3 代数展开：`expand`
函数签名：
```rust
pub fn expand<'a, C>(
    mechanics: impl IntoIterator<Item = (C, &'a Mechanic)>,
    ctx: &Context,
) -> Expansion<C>
```
将每个机制展开为单一连续变量，流出为正（$+$），流入/消耗为负（$-$），时间单位严格归一化为 **每秒（per second）**。

---

## 4. 关键计算与业务管线解析

### 管线 A：工厂求解管线（Solve Pipeline）
1. **数据准备**：从 `FactoryDocument` 收集所有启用的 `Mechanic` 与 `ExternalInput`；
2. **机制展开**：调用 `metatorio_core::expand` 生成变量及其对应的 `Flow` 向量；
3. **LP 建模**：
   - 目标函数：$\min \sum c_i x_i$（各机器初始投资/运行成本估算 + 外部输入加权）；
   - 约束行：每个 `DualVar` 的代数和满足供给/消耗平衡（严格供给模式下：$\sum \text{Output} - \sum \text{Input} \ge \text{Target}$）；
4. **后端求解**：派发至当前项目选定的 LP 后端（`HiGHS`、`SoPlex`、`Clarabel` 等）；
5. **诊断与回写**：若求解失败，提取无供给（`no_provider`）或无消耗（`no_consumer`）的流标识，生成结构化 `Diagnostic`。

### 管线 B：多星球宇宙工厂联合求解（Universe Factory）
- 普通工厂中隐式环境是单一的；宇宙工厂中每个本地机制由 `LocatedMechanic::Local { location, mechanic }` 包装；
- 引入星际物流机制：
  - `RocketExport`：消耗行星本地 $\text{RocketWeightCapacity}$ 和物品重量，在 `Space` 产出物品；
  - `OrbitalDrop`：在 `Space` 消耗物品，计入堆叠损耗与变质惩罚，投送到目的行星；
- 详见文档：[docs/universe-factory.md](docs/universe-factory.md)。

### 管线 C：全排列组合自动规划（Auto-Plan Pipeline）
位于 `crates/metatorio-runtime/src/auto_plan.rs`：
1. **可达性过滤**：通过 `Accessibility` 计算当前科技树与里程碑下可用的配方、机器与插件；
2. **组合全枚举**：枚举候选组合：$\text{Recipe} \times \text{Machine} \times \text{ModuleCombinations} \times \text{Beacons} \times \text{Quality}$；
3. **联合单步求解**：构建超大规模稀疏 LP，一次性求解最优组合；
4. **非零候选回写**：提取解向量中系数非零的真实生产单元（调用 `used_candidates`），自动生成工厂机制列表。

### 管线 D：类型化 IPC 与合约同步（Type-Safe IPC）
- Rust 侧暴露的命令集中登记在 `metatorio-app/src-tauri/src/bindings.rs`；
- 所有参与 IPC 传输的 DTO 必须派生 `#[cfg_attr(feature = "specta", derive(specta::Type))]`；
- 执行 `pnpm bindings` 后，`tauri-specta` 会自动将 Rust 类型与命令生成至 `metatorio-app/src/lib/runtime/bindings.ts`；
- 详见文档：[docs/typed-ipc.md](docs/typed-ipc.md)。

---

## 5. 开发调试与测试命令速查

所有命令请在工作区根目录或指定目录下运行：

```sh
# 1. 求解器内核单测（纯 Rust 单纯形模式，无需 C++ 工具链，耗时极短）
cargo test -p metatorio-solver --no-default-features --features microlp,rust-lp

# 2. 核心领域物理层单测
cargo test -p metatorio-core

# 3. 运行完整工作区 Rust 测试（包含全部集成测试与 Doc-tests，需 C++ 编译器）
cargo test --workspace

# 4. 纯 Rust 模式全工作区测试（当本地环境缺少 CMake/C++ 工具链时）
cargo test --workspace --no-default-features --features clarabel,microlp,rust-lp

# 5. 星际物流与宇宙工厂回归测试
cargo test -p metatorio-runtime --test orbital_cargo

# 6. 前端代码质量与类型检查（在 metatorio-app/ 目录下）
cd metatorio-app
pnpm check

# 7. 重新生成并校验 IPC 契约绑定（修改了 Rust 命令或 DTO 时必须执行）
cd metatorio-app
pnpm bindings
pnpm bindings:check
```

---

## 6. 给后续 Agent 的关键铁律（Golden Rules）

> [!CAUTION]
> **红线 1：禁止在仓库根目录运行 `npm install`**
> 前端工程位于 `metatorio-app/` 子目录下。仓库根目录只属于 Cargo 工作区，绝对不要在根目录初始化 `package.json` 或生成 `node_modules`。

> [!WARNING]
> **红线 2：严禁手写或篡改 `bindings.ts`**
> `metatorio-app/src/lib/runtime/bindings.ts` 是完全由 `tauri-specta` 生成的文件。若需修改前后端交互数据，请在 Rust 端修改结构体并在 `bindings.rs` 注册，随后运行 `pnpm bindings`。

> [!IMPORTANT]
> **红线 3：保持核心 crate 的无 GUI 纯净性**
> 任何在 `metatorio-core`、`metatorio-solver`、`metatorio-runtime` 中引入 Tauri、Svelte、GTK 或 Webview 相关依赖的行为均属严重违规。纯计算层必须保持无副作用与高可复用性。

> [!NOTE]
> **红线 4：Rust 工具链与 Windows 原生构建规则**
> - 由于 `metatorio-data` 使用了 `#![feature(mut_restriction)]`，Rust 构建需使用 **nightly** 工具链（与 CI 一致）。
> - Windows 下默认构建依赖 C++17、CMake 与 Ninja。若遇到生成器不匹配报错，优先在当前会话设置 `$env:CMAKE_GENERATOR = 'Ninja'`，详见 [docs/windows-native-build.md](docs/windows-native-build.md)。
