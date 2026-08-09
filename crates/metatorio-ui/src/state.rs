//! Application state and reducer.

use std::collections::BTreeMap;

use metatorio_core::{
    BoilerMechanic, Context, Expansion, GeneratorMechanic, IdWithQuality, ItemFuelMechanic,
    ItemLaunchMechanic, Mechanic, MiningMechanic, PlantMechanic, ReactorMechanic, RecipeMechanic,
    SpoilMechanic,
};

use crate::message::{
    AppMessage, BoilerMessage, Command, GeneratorMessage, ItemFuelMessage, ItemLaunchMessage,
    MechanicId, MechanicKind, MechanicMessage, MiningMessage, PlantMessage, ReactorMessage,
    RecipeMessage, SpoilMessage,
};

#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub factory: FactoryState,
    pub ui: UiState,
}

impl AppState {
    /// Apply one user intent and return side effects for the outer runtime.
    pub fn update(&mut self, message: AppMessage) -> Vec<Command> {
        match message {
            AppMessage::SetFactoryName(name) => {
                self.factory.name = name;
                vec![Command::Persist]
            }
            AppMessage::AddMechanic(kind) => {
                let Some(id) = self.factory.add(kind) else {
                    return Vec::new();
                };
                self.ui.selected = Some(id);
                self.ui.expanded.insert(id, true);
                vec![Command::Recompute, Command::Persist]
            }
            AppMessage::RemoveMechanic(id) => {
                self.factory.remove(id);
                self.ui.expanded.remove(&id);
                if self.ui.selected == Some(id) {
                    self.ui.selected = self.factory.mechanics.last().map(|entry| entry.id);
                }
                vec![Command::Recompute, Command::Persist]
            }
            AppMessage::SelectMechanic(id) => {
                self.ui.selected = id;
                Vec::new()
            }
            AppMessage::ToggleMechanic(id) => {
                let expanded = self.ui.expanded.entry(id).or_insert(true);
                *expanded = !*expanded;
                Vec::new()
            }
            AppMessage::Mechanic { id, message } => {
                let changed = self
                    .factory
                    .mechanics
                    .iter_mut()
                    .find(|entry| entry.id == id)
                    .is_some_and(|entry| entry.update(message));
                if changed {
                    vec![Command::Recompute, Command::Persist]
                } else {
                    Vec::new()
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct FactoryState {
    pub name: String,
    pub mechanics: Vec<MechanicEntry>,
    next_id: MechanicId,
}

impl Default for FactoryState {
    fn default() -> Self {
        Self {
            name: "Untitled factory".to_string(),
            mechanics: Vec::new(),
            next_id: 1,
        }
    }
}

impl FactoryState {
    pub fn add(&mut self, kind: MechanicKind) -> Option<MechanicId> {
        let mechanic = default_mechanic(kind)?;
        let id = self.next_id;
        self.next_id += 1;
        self.mechanics.push(MechanicEntry { id, mechanic });
        Some(id)
    }

    pub fn remove(&mut self, id: MechanicId) {
        self.mechanics.retain(|entry| entry.id != id);
    }

    /// Backend boundary: the UI state supplies ordered mechanics to core,
    /// while the core remains unaware of this crate's widgets and messages.
    pub fn expand(&self, ctx: &Context<'_>) -> Expansion<MechanicId> {
        metatorio_core::expand::expand(
            self.mechanics
                .iter()
                .map(|entry| (entry.id, &entry.mechanic)),
            ctx,
        )
    }
}

#[derive(Debug, Clone)]
pub struct MechanicEntry {
    pub id: MechanicId,
    pub mechanic: Mechanic,
}

impl MechanicEntry {
    pub fn kind(&self) -> MechanicKind {
        match &self.mechanic {
            Mechanic::Recipe(_) => MechanicKind::Recipe,
            Mechanic::Mining(_) => MechanicKind::Mining,
            Mechanic::Spoil(_) => MechanicKind::Spoil,
            Mechanic::Plant(_) => MechanicKind::Plant,
            Mechanic::ItemFuel(_) => MechanicKind::ItemFuel,
            Mechanic::ItemLaunch(_) => MechanicKind::ItemLaunch,
            Mechanic::Generator(_) => MechanicKind::Generator,
            Mechanic::Boiler(_) => MechanicKind::Boiler,
            Mechanic::Reactor(_) => MechanicKind::Reactor,
            _ => MechanicKind::Unsupported,
        }
    }

    /// Return whether the message belonged to this mechanic and changed it.
    pub fn update(&mut self, message: MechanicMessage) -> bool {
        match (&mut self.mechanic, message) {
            (Mechanic::Recipe(mechanic), MechanicMessage::Recipe(message)) => {
                update_recipe(mechanic, message)
            }
            (Mechanic::Mining(mechanic), MechanicMessage::Mining(message)) => {
                update_mining(mechanic, message)
            }
            (Mechanic::Spoil(mechanic), MechanicMessage::Spoil(message)) => {
                update_spoil(mechanic, message)
            }
            (Mechanic::Plant(mechanic), MechanicMessage::Plant(message)) => {
                update_plant(mechanic, message)
            }
            (Mechanic::ItemFuel(mechanic), MechanicMessage::ItemFuel(message)) => {
                update_item_fuel(mechanic, message)
            }
            (Mechanic::ItemLaunch(mechanic), MechanicMessage::ItemLaunch(message)) => {
                update_item_launch(mechanic, message)
            }
            (Mechanic::Generator(mechanic), MechanicMessage::Generator(message)) => {
                update_generator(mechanic, message)
            }
            (Mechanic::Boiler(mechanic), MechanicMessage::Boiler(message)) => {
                update_boiler(mechanic, message)
            }
            (Mechanic::Reactor(mechanic), MechanicMessage::Reactor(message)) => {
                update_reactor(mechanic, message)
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct UiState {
    pub selected: Option<MechanicId>,
    pub expanded: BTreeMap<MechanicId, bool>,
}

fn default_mechanic(kind: MechanicKind) -> Option<Mechanic> {
    Some(match kind {
        MechanicKind::Recipe => Mechanic::Recipe(RecipeMechanic::default()),
        MechanicKind::Mining => Mechanic::Mining(MiningMechanic::default()),
        MechanicKind::Spoil => Mechanic::Spoil(SpoilMechanic::default()),
        MechanicKind::Plant => Mechanic::Plant(PlantMechanic::default()),
        MechanicKind::ItemFuel => Mechanic::ItemFuel(ItemFuelMechanic::default()),
        MechanicKind::ItemLaunch => Mechanic::ItemLaunch(ItemLaunchMechanic::default()),
        MechanicKind::Generator => Mechanic::Generator(GeneratorMechanic::default()),
        MechanicKind::Boiler => Mechanic::Boiler(BoilerMechanic::default()),
        MechanicKind::Reactor => Mechanic::Reactor(ReactorMechanic::default()),
        MechanicKind::Unsupported => return None,
    })
}

fn set_id(id: &mut IdWithQuality, value: String) -> bool {
    if id.id == value {
        false
    } else {
        id.id = value;
        true
    }
}

fn set_quality(id: &mut IdWithQuality, value: String) -> bool {
    if id.quality == value {
        false
    } else {
        id.quality = value;
        true
    }
}

fn set_optional(current: &mut Option<String>, value: String) -> bool {
    let next = (!value.is_empty()).then_some(value);
    if *current == next {
        false
    } else {
        *current = next;
        true
    }
}

fn update_recipe(mechanic: &mut RecipeMechanic, message: RecipeMessage) -> bool {
    match message {
        RecipeMessage::RecipeId(value) => set_id(&mut mechanic.recipe, value),
        RecipeMessage::RecipeQuality(value) => set_quality(&mut mechanic.recipe, value),
        RecipeMessage::MachineId(value) => set_id(&mut mechanic.machine, value),
        RecipeMessage::MachineQuality(value) => set_quality(&mut mechanic.machine, value),
        RecipeMessage::Fuel(value) => set_optional(&mut mechanic.fuel, value),
        RecipeMessage::ClearFuel => mechanic.fuel.take().is_some(),
        RecipeMessage::FuelTemperature(value) => {
            if mechanic.fuel_temperature == value {
                false
            } else {
                mechanic.fuel_temperature = value;
                true
            }
        }
    }
}

fn update_mining(mechanic: &mut MiningMechanic, message: MiningMessage) -> bool {
    match message {
        MiningMessage::Resource(value) => {
            if mechanic.resource == value {
                false
            } else {
                mechanic.resource = value;
                true
            }
        }
        MiningMessage::MachineId(value) => set_id(&mut mechanic.machine, value),
        MiningMessage::MachineQuality(value) => set_quality(&mut mechanic.machine, value),
        MiningMessage::Fuel(value) => set_optional(&mut mechanic.fuel, value),
        MiningMessage::ClearFuel => mechanic.fuel.take().is_some(),
        MiningMessage::FuelTemperature(value) => {
            if mechanic.fuel_temperature == value {
                false
            } else {
                mechanic.fuel_temperature = value;
                true
            }
        }
    }
}

fn update_spoil(mechanic: &mut SpoilMechanic, message: SpoilMessage) -> bool {
    match message {
        SpoilMessage::ItemId(value) => set_id(&mut mechanic.item, value),
        SpoilMessage::ItemQuality(value) => set_quality(&mut mechanic.item, value),
    }
}

fn update_plant(mechanic: &mut PlantMechanic, message: PlantMessage) -> bool {
    match message {
        PlantMessage::SeedId(value) => set_id(&mut mechanic.seed, value),
        PlantMessage::SeedQuality(value) => set_quality(&mut mechanic.seed, value),
    }
}

fn update_item_fuel(mechanic: &mut ItemFuelMechanic, message: ItemFuelMessage) -> bool {
    match message {
        ItemFuelMessage::ItemId(value) => set_id(&mut mechanic.item, value),
        ItemFuelMessage::ItemQuality(value) => set_quality(&mut mechanic.item, value),
    }
}

fn update_item_launch(mechanic: &mut ItemLaunchMechanic, message: ItemLaunchMessage) -> bool {
    match message {
        ItemLaunchMessage::ItemId(value) => set_id(&mut mechanic.item, value),
        ItemLaunchMessage::ItemQuality(value) => set_quality(&mut mechanic.item, value),
        ItemLaunchMessage::WeightMode(value) => {
            if mechanic.weight_mode == value {
                false
            } else {
                mechanic.weight_mode = value;
                true
            }
        }
    }
}

fn update_generator(mechanic: &mut GeneratorMechanic, message: GeneratorMessage) -> bool {
    match message {
        GeneratorMessage::GeneratorId(value) => set_id(&mut mechanic.generator, value),
        GeneratorMessage::GeneratorQuality(value) => set_quality(&mut mechanic.generator, value),
        GeneratorMessage::Fluid(value) => {
            if mechanic.fluid == value {
                false
            } else {
                mechanic.fluid = value;
                true
            }
        }
        GeneratorMessage::Temperature(value) => {
            if mechanic.temperature == value {
                false
            } else {
                mechanic.temperature = value;
                true
            }
        }
    }
}

fn update_boiler(mechanic: &mut BoilerMechanic, message: BoilerMessage) -> bool {
    match message {
        BoilerMessage::BoilerId(value) => set_id(&mut mechanic.boiler, value),
        BoilerMessage::BoilerQuality(value) => set_quality(&mut mechanic.boiler, value),
        BoilerMessage::Fluid(value) => {
            if mechanic.fluid == value {
                false
            } else {
                mechanic.fluid = value;
                true
            }
        }
        BoilerMessage::Temperature(value) => {
            if mechanic.temperature == value {
                false
            } else {
                mechanic.temperature = value;
                true
            }
        }
        BoilerMessage::Fuel(value) => set_optional(&mut mechanic.fuel, value),
        BoilerMessage::ClearFuel => mechanic.fuel.take().is_some(),
        BoilerMessage::FuelTemperature(value) => {
            if mechanic.fuel_temperature == value {
                false
            } else {
                mechanic.fuel_temperature = value;
                true
            }
        }
    }
}

fn update_reactor(mechanic: &mut ReactorMechanic, message: ReactorMessage) -> bool {
    match message {
        ReactorMessage::ReactorId(value) => set_id(&mut mechanic.reactor, value),
        ReactorMessage::ReactorQuality(value) => set_quality(&mut mechanic.reactor, value),
        ReactorMessage::Neighbours(value) => {
            if mechanic.neighbours == value {
                false
            } else {
                mechanic.neighbours = value;
                true
            }
        }
        ReactorMessage::Fuel(value) => set_optional(&mut mechanic.fuel, value),
        ReactorMessage::ClearFuel => mechanic.fuel.take().is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_update_are_message_driven() {
        let mut state = AppState::default();
        let commands = state.update(AppMessage::AddMechanic(MechanicKind::Recipe));
        assert_eq!(state.factory.mechanics.len(), 1);
        assert!(commands.contains(&Command::Recompute));

        let id = state.factory.mechanics[0].id;
        state.update(AppMessage::Mechanic {
            id,
            message: MechanicMessage::Recipe(RecipeMessage::RecipeId("iron-plate".to_string())),
        });
        let Mechanic::Recipe(recipe) = &state.factory.mechanics[0].mechanic else {
            panic!("expected recipe");
        };
        assert_eq!(recipe.recipe.id, "iron-plate");
    }

    #[test]
    fn mechanic_ids_survive_reordering_by_removal() {
        let mut state = AppState::default();
        state.update(AppMessage::AddMechanic(MechanicKind::Recipe));
        state.update(AppMessage::AddMechanic(MechanicKind::Mining));
        let second_id = state.factory.mechanics[1].id;
        let first_id = state.factory.mechanics[0].id;

        state.update(AppMessage::RemoveMechanic(first_id));
        assert_eq!(state.factory.mechanics[0].id, second_id);
        assert_eq!(state.ui.selected, Some(second_id));
    }
}
