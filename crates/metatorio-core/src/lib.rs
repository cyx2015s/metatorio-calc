//! metatorio-core：与 GUI 框架解耦的核心数据与逻辑。
//!
//! 方向 B 第一步：把 metatorio-egui 的 11 个 typetag 机制（RecipeMechanic 等）
//! 合并为单个 `Mechanic` 枚举（`#[non_exhaustive]`，每个变体持有 1 个 struct，
//! 不内联结构体变体）。
//!
//! **`Mechanic` 表示工厂的 1 个组件（单个生产单元）**，不是列表：
//! 如 `Mechanic::Recipe(RecipeMechanic)` 即 1 个配方 + 1 个机器 + 可选燃料 +
//! 可选插件配置；工厂整体是用户层的 `Vec<Mechanic>`。
//!
//! 当前只承载**纯数据**：UI 状态（suggestion_*）、求解逻辑（AsFlow）与偏好配置
//! （machine_preferences/enumerate_*）均不在此层。

use serde::{Deserialize, Serialize};

/// 带品质的标识（物品/实体/配方/机器……）。
///
/// 第二元素是品质的**字符串名**（如 `"normal"`/`"uncommon"`），而非 u8 索引——
/// 跨模组迁移时自定义品质名无法映射到固定索引，故弃用 `(String, u8)` 形态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IdWithQuality(pub String, pub String);

/// 常规品质名。
pub const NORMAL_QUALITY: &str = "normal";

impl From<&str> for IdWithQuality {
    fn from(s: &str) -> Self {
        IdWithQuality(s.to_string(), NORMAL_QUALITY.to_string())
    }
}

impl From<String> for IdWithQuality {
    fn from(s: String) -> Self {
        IdWithQuality(s, NORMAL_QUALITY.to_string())
    }
}

// ── 模块配置（ModuleConfig 体系，纯数据）─────────────────────────

/// 一个机器实例的模块/信标配置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModuleConfig {
    pub modules: Vec<IdWithQuality>,
    pub beacons: Vec<BeaconConfig>,
}

/// 单个信标（插件塔）的配置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BeaconConfig {
    /// 这种插件塔中的模块（数量是塔内模块数，不是塔数量）。
    pub modules: Vec<(IdWithQuality, usize)>,
    /// 插件塔本身。
    pub beacon: IdWithQuality,
    /// 插件塔的数量。
    pub count: usize,
    /// 插件塔共享比例：值为 x 表示平均一个插件塔能覆盖到 x 个机器，
    /// 计算插件塔的耗电时需要除以相应的数量。
    pub share: f64,
}

// ── Mechanic 枚举 ────────────────────────────────────────────────
// 工厂的一个组件（单例配置）。每个变体持有 1 个 struct（不内联）。
// #[non_exhaustive]：机制集合会继续扩展，禁止外部穷尽匹配。

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Mechanic {
    Recipe(RecipeMechanic),
    Mining(MiningMechanic),
    Spoil(SpoilMechanic),
    Plant(PlantMechanic),
    ItemFuel(ItemFuelMechanic),
    ItemLaunch(ItemLaunchMechanic),
    Generator(GeneratorMechanic),
    Boiler(BoilerMechanic),
    FluidFuel(FluidFuelMechanic),
    FluidHeat(FluidHeatMechanic),
    Reactor(ReactorMechanic),
}

// ── 组件配置 struct（迁移自 metatorio-egui 的 XxxInstance，单例语义）──

/// 配方组件：在机器中按模块配置生产指定配方。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecipeMechanic {
    pub recipe: IdWithQuality,
    pub machine: IdWithQuality,
    pub module_config: ModuleConfig,
    /// 燃料：机器能源为 Fluid 时是 (流体名, 温度)；Burner 时是物品燃料；
    /// Electric/Heat/Void 时无效（None）。
    pub fuel: Option<(String, i32)>,
}

/// 采矿组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MiningMechanic {
    pub resource: String,
    pub machine: IdWithQuality,
    pub module_config: ModuleConfig,
    pub fuel: Option<IdWithQuality>,
}

/// 腐坏组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpoilMechanic {
    pub item: IdWithQuality,
}

/// 种植组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlantMechanic {
    pub seed: IdWithQuality,
}

/// 物品燃料组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemFuelMechanic {
    pub item: IdWithQuality,
}

/// 物品发射（火箭运力）组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemLaunchMechanic {
    pub item: IdWithQuality,
    /// true = 重量火箭（RocketWeightCapacity），false = 堆叠火箭（RocketSlotCapacity）。
    pub weight_mode: bool,
}

/// 发电机组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneratorMechanic {
    pub generator: IdWithQuality,
    pub fluid: String,
    pub temperature: i32,
}

/// 锅炉组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoilerMechanic {
    pub boiler: IdWithQuality,
    pub fluid: String,
    pub temperature: i32,
    pub fuel: Option<(String, i32)>,
}

/// 流体燃料组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FluidFuelMechanic {
    pub fluid: String,
    pub temperature: i32,
}

/// 流体热源组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FluidHeatMechanic {
    pub fluid: String,
    pub temperature: i32,
}

/// 反应堆组件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReactorMechanic {
    pub reactor: IdWithQuality,
    pub neighbours: u8,
    pub fuel: Option<(String, i32)>,
}
