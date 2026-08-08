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

use serde::{Deserialize, Serialize};

use crate::id::IdWithQuality;

/// 流标识。
///
/// `Flow<DualVar>` 中每个键代表一种流，值为流量。
#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
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
    /// 物品燃料流（**数值单位 = 焦耳 J**）。
    ///
    /// `category`：燃料类别；`has_burnt_result`：燃料是否带燃尽产物。
    /// 带燃尽产物物品栏的机器只接受 `true` 的燃料流；`false`（无燃尽产物）
    /// 可隐式转换为 `true`——子类型提升。
    ItemFuel {
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
                | DualVar::ItemFuel { .. }
        )
    }
}
