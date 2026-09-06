//! 带品质的标识（物品/实体/配方/机器……）。

use serde::{Deserialize, Serialize};

/// 带品质的标识。
///
/// 品质用**字符串名**（如 `"normal"`/`"uncommon"`），而非 u8 索引——
/// 跨模组迁移时自定义品质名无法映射到固定索引，故弃用 `(String, u8)` 形态。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IdWithQuality {
    pub id: String,
    pub quality: String,
}

impl IdWithQuality {
    pub fn new(id: impl Into<String>, quality: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            quality: quality.into(),
        }
    }
}

/// 缺省品质为 `"normal"`（空 id + normal 品质），而不是空品质——
/// 空品质会让 UI 显示不出角标、求解按 level 0 处理，语义上等同 normal
/// 却多一个"空"状态。所有 `Default` 构造的机制字段（机器/配方/插件塔/
/// 插件）都因此带上 normal 品质。
impl Default for IdWithQuality {
    fn default() -> Self {
        Self {
            id: String::new(),
            quality: NORMAL_QUALITY.to_string(),
        }
    }
}

/// 常规品质名。
pub const NORMAL_QUALITY: &str = "normal";

impl From<&str> for IdWithQuality {
    fn from(s: &str) -> Self {
        IdWithQuality::new(s, NORMAL_QUALITY)
    }
}

impl From<String> for IdWithQuality {
    fn from(s: String) -> Self {
        IdWithQuality::new(s, NORMAL_QUALITY)
    }
}
