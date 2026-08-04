//! 带品质的标识（物品/实体/配方/机器……）。

use serde::{Deserialize, Serialize};

/// 带品质的标识。
///
/// 品质用**字符串名**（如 `"normal"`/`"uncommon"`），而非 u8 索引——
/// 跨模组迁移时自定义品质名无法映射到固定索引，故弃用 `(String, u8)` 形态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
