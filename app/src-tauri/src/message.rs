//! UI intent types.
//!
//! These enums are the boundary between rendering and application state. They
//! intentionally do not mention egui, so the same reducer can later be used by
//! another frontend.

use std::fmt;

use serde::{Deserialize, Serialize};

pub type MechanicId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MechanicKind {
    Recipe,
    Mining,
    Spoil,
    Plant,
    ItemFuel,
    ItemLaunch,
    Generator,
    Boiler,
    Reactor,
    Unsupported,
}

impl MechanicKind {
    pub const ALL: [Self; 9] = [
        Self::Recipe,
        Self::Mining,
        Self::Spoil,
        Self::Plant,
        Self::ItemFuel,
        Self::ItemLaunch,
        Self::Generator,
        Self::Boiler,
        Self::Reactor,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Recipe => "Recipe",
            Self::Mining => "Mining",
            Self::Spoil => "Spoil",
            Self::Plant => "Plant",
            Self::ItemFuel => "Item fuel",
            Self::ItemLaunch => "Item launch",
            Self::Generator => "Generator",
            Self::Boiler => "Boiler",
            Self::Reactor => "Reactor",
            Self::Unsupported => "Unsupported",
        }
    }
}

impl fmt::Display for MechanicKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum MechanicMessage {
    Recipe(RecipeMessage),
    Mining(MiningMessage),
    Spoil(SpoilMessage),
    Plant(PlantMessage),
    ItemFuel(ItemFuelMessage),
    ItemLaunch(ItemLaunchMessage),
    Generator(GeneratorMessage),
    Boiler(BoilerMessage),
    Reactor(ReactorMessage),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum RecipeMessage {
    RecipeId(#[serde(rename = "recipe_id")] String),
    RecipeQuality(#[serde(rename = "recipe_quality")] String),
    MachineId(#[serde(rename = "machine_id")] String),
    MachineQuality(#[serde(rename = "machine_quality")] String),
    Fuel(#[serde(rename = "fuel")] String),
    ClearFuel,
    FuelTemperature(#[serde(rename = "fuel_temperature")] Option<i32>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum MiningMessage {
    Resource(#[serde(rename = "resource")] String),
    MachineId(#[serde(rename = "machine_id")] String),
    MachineQuality(#[serde(rename = "machine_quality")] String),
    Fuel(#[serde(rename = "fuel")] String),
    ClearFuel,
    FuelTemperature(#[serde(rename = "fuel_temperature")] Option<i32>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SpoilMessage {
    ItemId(#[serde(rename = "item_id")] String),
    ItemQuality(#[serde(rename = "item_quality")] String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PlantMessage {
    SeedId(#[serde(rename = "seed_id")] String),
    SeedQuality(#[serde(rename = "seed_quality")] String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ItemFuelMessage {
    ItemId(#[serde(rename = "item_id")] String),
    ItemQuality(#[serde(rename = "item_quality")] String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ItemLaunchMessage {
    ItemId(#[serde(rename = "item_id")] String),
    ItemQuality(#[serde(rename = "item_quality")] String),
    WeightMode(#[serde(rename = "weight_mode")] bool),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum GeneratorMessage {
    GeneratorId(#[serde(rename = "generator_id")] String),
    GeneratorQuality(#[serde(rename = "generator_quality")] String),
    Fluid(#[serde(rename = "fluid")] String),
    Temperature(#[serde(rename = "temperature")] Option<i32>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum BoilerMessage {
    BoilerId(#[serde(rename = "boiler_id")] String),
    BoilerQuality(#[serde(rename = "boiler_quality")] String),
    Fluid(#[serde(rename = "fluid")] String),
    Temperature(#[serde(rename = "temperature")] Option<i32>),
    Fuel(#[serde(rename = "fuel")] String),
    ClearFuel,
    FuelTemperature(#[serde(rename = "fuel_temperature")] Option<i32>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ReactorMessage {
    ReactorId(#[serde(rename = "reactor_id")] String),
    ReactorQuality(#[serde(rename = "reactor_quality")] String),
    Neighbours(#[serde(rename = "neighbours")] u8),
    Fuel(#[serde(rename = "fuel")] String),
    ClearFuel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AppMessage {
    SetFactoryName(#[serde(rename = "name")] String),
    AddMechanic(#[serde(rename = "kind")] MechanicKind),
    RemoveMechanic(#[serde(rename = "id")] MechanicId),
    SelectMechanic(#[serde(rename = "id")] Option<MechanicId>),
    ToggleMechanic(#[serde(rename = "id")] MechanicId),
    Mechanic {
        id: MechanicId,
        message: MechanicMessage,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Recompute,
    Persist,
}
