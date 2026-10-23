//! Project-level LP backend selection.
//!
//! This type lives in the solver crate so the document model, MCP schema and
//! the LP adapter all use one spelling. Availability depends on the Cargo
//! features compiled into the binary; an unavailable choice is reported as a
//! solve error instead of being silently replaced by another backend.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LpSolver {
    Clarabel,
    Highs,
    Microlp,
    /// Legacy Rust path. The serialized id remains `rust-certified` so existing
    /// project files keep loading; `rust-based` is accepted as an alias.
    #[serde(rename = "rust-certified", alias = "rust-based")]
    RustBased,
}

impl Default for LpSolver {
    fn default() -> Self {
        // Prefer a mature solver whenever it is compiled in; fall back to the
        // legacy Rust path only in a Rust-only build.
        if cfg!(feature = "highs") {
            Self::Highs
        } else if cfg!(feature = "clarabel") {
            Self::Clarabel
        } else if cfg!(feature = "microlp") {
            Self::Microlp
        } else {
            Self::RustBased
        }
    }
}

impl LpSolver {
    pub const ALL: [Self; 4] = [Self::Clarabel, Self::Highs, Self::Microlp, Self::RustBased];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clarabel => "clarabel",
            Self::Highs => "highs",
            Self::Microlp => "microlp",
            Self::RustBased => "rust-certified",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Clarabel => "Clarabel",
            Self::Highs => "HiGHS",
            Self::Microlp => "microlp",
            Self::RustBased => "Rust based",
        }
    }

    pub const fn is_available(self) -> bool {
        match self {
            Self::Clarabel => cfg!(feature = "clarabel"),
            Self::Highs => cfg!(feature = "highs"),
            Self::Microlp => cfg!(feature = "microlp"),
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
            "rust-certified" | "rust-based" => Ok(Self::RustBased),
            _ => Err(format!("unknown LP solver: {value}")),
        }
    }
}
