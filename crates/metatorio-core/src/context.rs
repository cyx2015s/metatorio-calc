//! 展开上下文：原型数据（不可变）+ 游戏进程状态（可变）。

use ahash::RandomState;
use indexmap::IndexMap;
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

impl Context<'_> {
    pub fn new<'a>(prototype: &'a PrototypeStore, game: &'a GameState) -> Context<'a> {
        Context { prototype, game }
    }
}

/// 游戏进程状态（暂定名）：实时加成、解锁进度等会话级数据。
///
/// 与原型数据分离（原型不可变、进程可变）；默认值 = "无加成、仅 normal 品质"。
#[derive(Debug, Clone)]
pub struct GameState {
    pub reactor_neighbours: std::collections::BTreeMap<String, u8>,
    /// 品质列表（索引 = 品质等级，0 = normal；含解锁上限）。
    pub qualities: Vec<String>,
    /// 解锁的品质上限索引（0 = normal）。
    pub max_quality: usize,
    /// 配方名 → 额外产能加成（科技等解锁，第一版未启用）。
    pub recipe_productivity: IndexMap<String, f64, RandomState>,
    /// 采矿产能加成（第一版未启用）。
    pub mining_productivity: f64,
    /// 当前规划环境中的太阳能倍率（星球/地表属性已由 runtime 注入）。
    pub solar_power_multiplier: f64,
    /// 当前规划环境的昼夜周期（tick，60 tick = 1 s）。
    pub day_night_cycle: f64,
    /// 显式轨道表面持续满日照，不需要昼夜储能。
    pub solar_continuous_daylight: bool,
    /// 当前地面星球要求实体供暖；轨道环境始终为 false。
    pub entities_require_heating: bool,
    /// 当前表面属性，供使用自定义 solar_coefficient_property 的面板查询。
    pub surface_properties: std::collections::BTreeMap<String, f64>,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            reactor_neighbours: Default::default(),
            qualities: vec!["normal".to_string()],
            max_quality: 0,
            recipe_productivity: IndexMap::with_hasher(RandomState::default()),
            mining_productivity: 0.0,
            solar_power_multiplier: 1.0,
            day_night_cycle: 25_200.0,
            solar_continuous_daylight: false,
            entities_require_heating: false,
            surface_properties: Default::default(),
        }
    }
}

impl GameState {
    /// 品质名 → 等级索引（未解锁/未知品质 → 0 = normal）。
    pub fn quality_level(&self, quality: &str) -> usize {
        self.qualities
            .iter()
            .position(|q| q == quality)
            .unwrap_or(0)
    }
}
