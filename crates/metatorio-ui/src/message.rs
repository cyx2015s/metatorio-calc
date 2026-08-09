//! UI intent types.
//!
//! These enums are the boundary between rendering and application state. They
//! intentionally do not mention egui, so the same reducer can later be used by
//! another frontend.

use std::fmt;

pub type MechanicId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecipeMessage {
    RecipeId(String),
    RecipeQuality(String),
    MachineId(String),
    MachineQuality(String),
    Fuel(String),
    ClearFuel,
    FuelTemperature(Option<i32>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MiningMessage {
    Resource(String),
    MachineId(String),
    MachineQuality(String),
    Fuel(String),
    ClearFuel,
    FuelTemperature(Option<i32>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpoilMessage {
    ItemId(String),
    ItemQuality(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlantMessage {
    SeedId(String),
    SeedQuality(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemFuelMessage {
    ItemId(String),
    ItemQuality(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemLaunchMessage {
    ItemId(String),
    ItemQuality(String),
    WeightMode(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratorMessage {
    GeneratorId(String),
    GeneratorQuality(String),
    Fluid(String),
    Temperature(Option<i32>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoilerMessage {
    BoilerId(String),
    BoilerQuality(String),
    Fluid(String),
    Temperature(Option<i32>),
    Fuel(String),
    ClearFuel,
    FuelTemperature(Option<i32>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactorMessage {
    ReactorId(String),
    ReactorQuality(String),
    Neighbours(u8),
    Fuel(String),
    ClearFuel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppMessage {
    SetFactoryName(String),
    AddMechanic(MechanicKind),
    RemoveMechanic(MechanicId),
    SelectMechanic(Option<MechanicId>),
    ToggleMechanic(MechanicId),
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
