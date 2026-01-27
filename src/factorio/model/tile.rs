use crate::factorio::{AutoplaceSpecification, HasPrototypeBase, PrototypeBase};

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct TilePrototype {
    #[serde(flatten)]
    pub base: PrototypeBase,
    #[serde(default)]
    pub fluid: Option<String>,
    #[serde(default)]
    pub autoplace: Option<AutoplaceSpecification>,
}

impl HasPrototypeBase for TilePrototype {
    fn base(&self) -> &PrototypeBase {
        &self.base
    }
}
