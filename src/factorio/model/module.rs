use serde::Deserialize;

use crate::factorio::common::{Effect, PrototypeBase};

#[derive(Debug, Clone, Deserialize)]
pub struct ModulePrototype {
    #[serde(flatten)]
    pub base: PrototypeBase,

    /// 增强效果
    pub effect: Effect,

    /// 可安装的机器类别
    pub category: String,

    /// 等级
    pub tier: f64,
}
