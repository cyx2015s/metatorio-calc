//! metatorio-core：异星工厂量化工具的核心领域模型与物理/机制展开引擎。
//!
//! 本 crate 是整个系统的纯领域层（Domain Core），完全与 GUI 框架、外壳和具体求解器解耦：
//! - **领域模型**：定义了生产单元 [`Mechanic`]（配方、采矿、反应堆、聚变、变质等）、流标识 [`DualVar`]、
//!   品质体系 [`IdWithQuality`]、跨星球空间坐标 [`Location`] 与 [`LocatedDualVar`]；
//! - **机制代数展开**：通过 [`expand`](crate::expand::expand) 将具体游戏机制转化为数学规划所需的
//!   原始变量（[`ExpandedVariable`]）与物质/能量代数流平衡向量（[`Flow`]）；
//! - **科技与可达性推导**：基于科技树图谱推导物品、配方、星球的先决条件与里程碑顺序（[`Accessibility`]）；
//! - **纯数据与确定性**：无全局状态、无 I/O 副作用，所有逻辑均基于不可变的 [`Context`] 原型上下文计算。

pub mod accessibility;
pub mod context;
pub mod dual_var;
pub mod energy;
pub mod expand;
pub mod id;
pub mod location;
pub mod mechanic;
pub mod planet;
pub mod prim_var;
pub mod productivity;
pub mod quality;
pub mod science;
pub mod temp_flow;

pub use accessibility::{
    Accessibility, AccessibilityOptions, Accessible, GraphData, Requirement, build_graph,
    compute_accessibility, compute_accessibility_with_graph, milestone_order,
    milestone_order_with_graph,
};
pub use context::{Context, GameState};
pub use dual_var::DualVar;
pub use expand::{SolarBalance, instance_cost, solar_balance};
pub use id::{IdWithQuality, NORMAL_QUALITY};
pub use location::{
    LocalLocatedMechanic, LocatedDualVar, LocatedMechanic, Location, OrbitalCargoMechanic,
};
pub use mechanic::{
    BeaconConfig, BoilerMechanic, FluidFuelMechanic, FluidHeatMechanic, Fuel,
    FusionGeneratorMechanic, FusionReactorMechanic, GeneratorMechanic, ItemFuelMechanic,
    ItemLaunchMechanic, Mechanic, MiningMechanic, ModuleConfig, PlantMechanic, ReactorMechanic,
    RecipeMechanic, SolarMechanic, SpoilMechanic, TileDisposeMechanic, TileExtractMechanic,
    VirtualScienceMechanic,
};
pub use prim_var::{AIndexMap, ExpandedVariable, Expansion, Flow};
pub use productivity::{InfiniteTechLevel, ProductivityResult, compute_productivity};
