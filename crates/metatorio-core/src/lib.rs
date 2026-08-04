//! metatorio-core：与 GUI 框架解耦的核心数据与逻辑。
//!
//! 方向 B：Mechanic 组件枚举（工厂的单个生产单元）+ DualVar（流标识）。
//! 纯数据层：UI 状态、求解逻辑（AsFlow）、偏好配置均不在此层。

pub mod dual_var;
pub mod id;
pub mod mechanic;

pub use dual_var::DualVar;
pub use id::{IdWithQuality, NORMAL_QUALITY};
pub use mechanic::{
    BeaconConfig, BoilerMechanic, GeneratorMechanic, ItemFuelMechanic, ItemLaunchMechanic,
    Mechanic, MiningMechanic, ModuleConfig, PlantMechanic, ReactorMechanic, RecipeMechanic,
    SpoilMechanic,
};
