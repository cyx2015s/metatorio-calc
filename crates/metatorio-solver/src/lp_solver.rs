//! 项目级 LP 后端选择。
//!
//! 定义在求解器 crate 内，使文档模型、MCP schema 与 LP 适配层共用同一拼写。
//! 可用性取决于二进制编译时启用的 Cargo feature；若所选后端未编译，
//! 直接报求解错误，而不静默替换为其他后端。
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LpSolver {
    Clarabel,
    Highs,
    Microlp,
    Soplex,
    /// 传统纯 Rust 认证后端。序列化 ID 仍保留 `rust-certified` 以兼容已有工程文件，
    /// 同时也接受别名 `rust-based`。
    #[serde(rename = "rust-certified", alias = "rust-based")]
    RustBased,
}

impl Default for LpSolver {
    fn default() -> Self {
        // 只要编译了成熟后端就优先作为默认选项；仅纯 Rust 构建时回落到旧版 Rust 后端。
        if cfg!(feature = "highs") {
            Self::Highs
        } else if cfg!(feature = "clarabel") {
            Self::Clarabel
        } else if cfg!(feature = "microlp") {
            Self::Microlp
        } else if cfg!(feature = "soplex") {
            Self::Soplex
        } else {
            Self::RustBased
        }
    }
}

impl LpSolver {
    pub const ALL: [Self; 5] = [
        Self::Clarabel,
        Self::Highs,
        Self::Microlp,
        Self::Soplex,
        Self::RustBased,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clarabel => "clarabel",
            Self::Highs => "highs",
            Self::Microlp => "microlp",
            Self::Soplex => "soplex",
            Self::RustBased => "rust-certified",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Clarabel => "Clarabel",
            Self::Highs => "HiGHS",
            Self::Microlp => "microlp",
            Self::Soplex => "SoPlex",
            Self::RustBased => "Rust based",
        }
    }

    pub const fn is_available(self) -> bool {
        match self {
            Self::Clarabel => cfg!(feature = "clarabel"),
            Self::Highs => cfg!(feature = "highs"),
            Self::Microlp => cfg!(feature = "microlp"),
            Self::Soplex => cfg!(feature = "soplex"),
            Self::RustBased => cfg!(feature = "rust-lp"),
        }
    }

    pub fn unavailable_reason(self) -> String {
        format!(
            "LP backend {} is unavailable in this build (missing Cargo feature)",
            self.label()
        )
    }
}

impl std::fmt::Display for LpSolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for LpSolver {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "clarabel" => Ok(Self::Clarabel),
            "highs" => Ok(Self::Highs),
            "microlp" => Ok(Self::Microlp),
            "soplex" => Ok(Self::Soplex),
            "rust-certified" | "rust-based" => Ok(Self::RustBased),
            _ => Err(format!("unknown LP solver: {value}")),
        }
    }
}
