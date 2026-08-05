//! 展开上下文：原型数据（不可变）+ 游戏进程状态（可变）。

use metatorio_data::store::PrototypeStore;

/// 展开上下文：求解/展开所需的全部外部数据的统一入口。
///
/// 依赖只通过这一个参数传入，新增依赖（星球属性、时间刻度……）
/// 扩展字段即可，不改变调用链。
#[derive(Debug)]
pub struct Context<'a> {
    /// 原型数据（加载后不可变 + 惰性派生）。
    pub prototype: &'a PrototypeStore,
    /// 游戏进程状态（会话级、可变）。
    pub game: &'a GameState,
}

impl<'a> Context<'a> {
    pub fn new(prototype: &'a PrototypeStore, game: &'a GameState) -> Self {
        Self { prototype, game }
    }
}

/// 游戏进程状态（暂定名）：实时加成、解锁进度等会话级数据。
///
/// 与原型数据分离（原型不可变、进程可变）；默认值 = "无加成、仅 normal 品质"。
#[derive(Debug, Clone, Default)]
pub struct GameState {
    /// 解锁的品质上限索引（0 = normal）。
    pub max_quality: usize,
    /// 配方名 → 额外产能加成（科技等解锁，第一版未启用）。
    pub recipe_productivity: indexmap::IndexMap<String, f64, ahash::RandomState>,
    /// 采矿产能加成（第一版未启用）。
    pub mining_productivity: f64,
}
