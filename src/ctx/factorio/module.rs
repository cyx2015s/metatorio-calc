use serde::Deserialize;

use crate::ctx::factorio::common::{Effect, PrototypeBase};

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ModulePrototype {
    #[serde(flatten)]
    pub(crate) base: PrototypeBase,

    /// 增强效果
    pub(crate) effect: Effect,

    /// 可安装的机器类别
    pub(crate) category: String,

    /// 等级
    pub(crate) tier: f64,
}
