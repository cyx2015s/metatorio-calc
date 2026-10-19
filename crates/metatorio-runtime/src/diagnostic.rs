//! Human-readable reasons plus machine-readable flow identities. Never require
//! a GUI or MCP consumer to recover objects from Display/Debug output.
use metatorio_core::DualVar;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub no_provider: Vec<DualVar>,
    #[serde(default)]
    pub no_consumer: Vec<DualVar>,
    #[serde(default)]
    pub flows: Vec<DualVar>,
}

impl Diagnostic {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            no_provider: Vec::new(),
            no_consumer: Vec::new(),
            flows: Vec::new(),
        }
    }
}

impl From<String> for Diagnostic {
    fn from(message: String) -> Self {
        Self::new("operation-failed", message)
    }
}
impl From<&str> for Diagnostic {
    fn from(message: &str) -> Self {
        Self::from(message.to_owned())
    }
}
impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Diagnostic {}

impl From<crate::RuntimeError> for Diagnostic {
    fn from(error: crate::RuntimeError) -> Self {
        match error {
            crate::RuntimeError::Diagnostic(diagnostic) => *diagnostic,
            other => Self::from(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_reference_fixtures_roundtrip_all_flow_variants() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/dual-var.json")).unwrap();
        let cases = cases.as_array().unwrap();
        assert_eq!(cases.len(), 15);
        for case in cases {
            let flow: DualVar = serde_json::from_value(case["flow"].clone()).unwrap();
            assert_eq!(serde_json::to_value(flow).unwrap(), case["flow"]);
        }
    }
}
