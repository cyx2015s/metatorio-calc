//! 流标识（DualVar）：工厂中一切可流动/守恒的东西的身份枚举。
//!
//! 命名来源：线性规划中配方是原始变量（PrimVar），物品守恒约束对应
//! 对偶变量（DualVar）——每个变体是一条守恒约束（流）的身份。
//!
//! 流体热量模型：
//! - `Fluid { name, temperature }`：带温度状态的流体本体
//! - `FluidHeat { filter }`：**纯筛选**的虚拟流体热量流（不含温度）。
//!   它只在机制明确需要抽象热量时显式加入；普通流体温度通过区间子类型
//!   和区间转换流表达。

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::id::IdWithQuality;

/// 流标识。
///
/// `Flow<DualVar>` 中每个键代表一种流，值为流量。
#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[repr(u8)]
#[non_exhaustive]
pub enum DualVar {
    #[default]
    Unknown,
    Item(IdWithQuality),
    /// 流体本体；温度区间键由展开层收敛为单点决策。
    Fluid {
        name: String,
        temperature: [i32; 2],
    },
    Entity(IdWithQuality),
    /// 无类型热量（核热等）。
    Heat,
    Electricity,
    /// 虚拟流体热量流（**数值单位 = 焦耳 J**）：纯筛选，不含温度。
    ///
    /// 由需要抽象热量的机制显式加入，数值单位为焦耳；
    FluidHeat {
        filter: String,
    },
    /// 虚拟流体燃料流（**数值单位 = 焦耳 J**）：纯筛选，不含温度。
    ///
    /// 由流体燃料机制（燃烧热值流体）显式加入；`filter` 为流体名，
    /// 空串表示"任意"。带 filter 的流可经零成本转换流归并为空串流。
    FluidFuel {
        filter: String,
    },
    /// 物品燃料需求流（**数值单位 = 焦耳 J**）：BurnerEnergySource / 用户流。
    ///
    /// `category`：机器接受的燃料类别集合；`has_burnt_result`：机器是否带
    /// 燃尽产物物品栏。燃料物品侧的供给见 `ItemFuelSupply`，二者由
    /// `add_conversion_flows` 按"类别集合有重叠"生成零成本转换。
    ItemFuel {
        category: Vec<String>,
        #[serde(default)]
        has_burnt_result: bool,
    },
    /// 物品燃料供给流（**数值单位 = 焦耳 J**）：燃料物品侧的燃料类别集合。
    ///
    /// 与需求侧 `ItemFuel` 分列，使"燃料与 BurnerEnergySource 的
    /// fuel_categories 只要有重叠即兼容"能建模为 `ItemFuelSupply →
    /// ItemFuel` 的单向零成本转换：供给侧不会成为中转，避免无关类别集合
    /// 经中间键互相桥接（重叠关系不可传递）。
    ItemFuelSupply {
        category: Vec<String>,
        #[serde(default)]
        has_burnt_result: bool,
    },
    /// 按堆叠数限制的火箭运力，1 单位 = 1 个槽位
    RocketSlotCapacity,
    /// 按重量限制的火箭运力，1 单位 = 1 重量单位
    RocketWeightCapacity,
    Pollution {
        name: String,
    },
    Custom {
        name: String,
    },
}

impl DualVar {
    pub fn is_energy(&self) -> bool {
        matches!(
            self,
            DualVar::Heat
                | DualVar::Electricity
                | DualVar::FluidHeat { .. }
                | DualVar::FluidFuel { .. }
                | DualVar::ItemFuel { .. }
                | DualVar::ItemFuelSupply { .. }
        )
    }
}
