use crate::factorio::{PrototypeBase, option_as_vec_or_empty};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TechnologyPrototype {
    #[serde(flatten)]
    pub base: PrototypeBase,

    #[serde(default)]
    pub essential: bool,
    #[serde(default)]
    pub max_level: Option<MaxLevel>,

    #[serde(default)]
    pub research_trigger: Option<ResearchTrigger>,

    #[serde(default, deserialize_with = "option_as_vec_or_empty")]
    pub effects: Option<Vec<Modifier>>,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
pub struct TechnologyUnit {
    #[serde(default)]
    pub ingredients: Vec<ResearchIngredient>,
    pub time: f64,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
pub struct ResearchIngredient(String, f64);

#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MaxLevel {
    #[default]
    Infinite,
    Finite(f64),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type")]
pub enum ResearchTrigger {
    MineEntity { entity: String },
    CraftItem {
        item: String,
    },
    CraftFluid {
        fluid: String,
    },
    SendItemToOrbit {
        item: String,
    },
    CaptureSpawner { entity: Option<String> },
    BuildEntity { entity: String },
    CreateSpacePlatform,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "kebab-case", tag = "type")]
pub enum Modifier {
    InserterStackSizeBonus(SimpleModifier),
    BulkInserterCapacityBonus(SimpleModifier),
    LaboratorySpeed(SimpleModifier),
    UnlockRecipe {
        recipe: String,
    },
    MiningDrillProductivityBonus(SimpleModifier),
    LaboratoryProductivity(SimpleModifier),
    UnlockSpaceLocation {
        space_location: String,
    },
    UnlockQuality {
        quality: String,
    },
    ChangeRecipeProductivity {
        recipe: String,
        change: f64,
    },
    MiningWithFluid(BoolModifier),
    BeaconDistribution(SimpleModifier),
    BeltStackSizeBonus(SimpleModifier),
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct SimpleModifier {
    modifier: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct BoolModifier {
    pub modifier: bool,
}
