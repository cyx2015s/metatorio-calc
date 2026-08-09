//! egui frontend for the message-driven application state.

use eframe::egui;
use metatorio_core::{IdWithQuality, Mechanic};

use crate::message::{
    AppMessage, BoilerMessage, Command, GeneratorMessage, ItemFuelMessage, ItemLaunchMessage,
    MechanicId, MechanicKind, MechanicMessage, MiningMessage, PlantMessage, ReactorMessage,
    RecipeMessage, SpoilMessage,
};
use crate::state::{AppState, MechanicEntry};

#[derive(Debug, Default)]
pub struct MetatorioApp {
    pub state: AppState,
    pub last_commands: Vec<Command>,
}

// These macros only remove widget-binding boilerplate. The reducer and the
// mechanic-specific message mapping remain explicit in `state.rs`.
macro_rules! mechanic_id_fields {
    (
        $ui:expr,
        $value:expr,
        $label:expr,
        $id:expr,
        $messages:expr,
        $wrap:path,
        $id_message:path,
        $quality_message:path $(,)?
    ) => {{
        id_fields(
            $ui,
            $value,
            $label,
            $id,
            |value| $wrap($id_message(value)),
            |value| $wrap($quality_message(value)),
            $messages,
        );
    }};
}

macro_rules! mechanic_text_field {
    ($ui:expr, $label:expr, $value:expr, $id:expr, $messages:expr, $wrap:path, $message:path $(,)?) => {{
        text_field(
            $ui,
            $label,
            $value,
            $id,
            |value| $wrap($message(value)),
            $messages,
        );
    }};
}

macro_rules! mechanic_optional_text_field {
    ($ui:expr, $label:expr, $value:expr, $id:expr, $messages:expr, $wrap:path, $message:path $(,)?) => {{
        optional_text_field(
            $ui,
            $label,
            $value,
            $id,
            |value| $wrap($message(value)),
            $messages,
        );
    }};
}

impl eframe::App for MetatorioApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut messages = Vec::new();
        render(ui, &self.state, &mut messages);
        self.last_commands.clear();
        for message in messages {
            self.last_commands.extend(self.state.update(message));
        }
        ui.request_repaint_after_secs(0.1);
    }
}

pub fn render(ui: &mut egui::Ui, state: &AppState, messages: &mut Vec<AppMessage>) {
    egui::Panel::top("toolbar").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Metatorio");
            ui.separator();
            let mut name = state.factory.name.clone();
            if ui.text_edit_singleline(&mut name).changed() {
                messages.push(AppMessage::SetFactoryName(name));
            }

            ui.menu_button("Add mechanic", |ui| {
                for kind in MechanicKind::ALL {
                    if ui.button(kind.label()).clicked() {
                        messages.push(AppMessage::AddMechanic(kind));
                        ui.close();
                    }
                }
            });
            ui.label(format!("{} mechanics", state.factory.mechanics.len()));
        });
    });

    egui::Panel::right("inspector")
        .default_size(230.0)
        .show(ui, |ui| render_inspector(ui, state));

    egui::CentralPanel::default().show(ui, |ui| {
        if state.factory.mechanics.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.heading("Start with a mechanic");
                ui.label("Use Add mechanic to create the first production unit.");
            });
            return;
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for entry in &state.factory.mechanics {
                    render_entry(ui, state, entry, messages);
                    ui.add_space(8.0);
                }
            });
    });
}

fn render_inspector(ui: &mut egui::Ui, state: &AppState) {
    ui.heading("Inspector");
    let Some(selected) = state.ui.selected else {
        ui.label("Select a mechanic to inspect it.");
        return;
    };
    let Some(entry) = state
        .factory
        .mechanics
        .iter()
        .find(|entry| entry.id == selected)
    else {
        ui.label("The selected mechanic no longer exists.");
        return;
    };

    ui.label(format!("#{}", entry.id));
    ui.label(entry.kind().label());
    ui.separator();
    ui.small("The inspector is intentionally read-only. Editing is handled by messages from the mechanic card.");
}

fn render_entry(
    ui: &mut egui::Ui,
    state: &AppState,
    entry: &MechanicEntry,
    messages: &mut Vec<AppMessage>,
) {
    let selected = state.ui.selected == Some(entry.id);
    let expanded = state.ui.expanded.get(&entry.id).copied().unwrap_or(true);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            if ui
                .selectable_label(selected, format!("#{}  {}", entry.id, entry.kind()))
                .clicked()
            {
                messages.push(AppMessage::SelectMechanic(Some(entry.id)));
            }
            if ui
                .small_button(if expanded { "Collapse" } else { "Expand" })
                .clicked()
            {
                messages.push(AppMessage::ToggleMechanic(entry.id));
            }
            if ui.small_button("Remove").clicked() {
                messages.push(AppMessage::RemoveMechanic(entry.id));
            }
        });

        if expanded {
            ui.separator();
            render_mechanic(ui, entry.id, &entry.mechanic, messages);
        }
    });
}

fn render_mechanic(
    ui: &mut egui::Ui,
    id: MechanicId,
    mechanic: &Mechanic,
    messages: &mut Vec<AppMessage>,
) {
    match mechanic {
        Mechanic::Recipe(mechanic) => {
            mechanic_id_fields!(
                ui,
                &mechanic.recipe,
                "Recipe",
                id,
                messages,
                MechanicMessage::Recipe,
                RecipeMessage::RecipeId,
                RecipeMessage::RecipeQuality,
            );
            mechanic_id_fields!(
                ui,
                &mechanic.machine,
                "Machine",
                id,
                messages,
                MechanicMessage::Recipe,
                RecipeMessage::MachineId,
                RecipeMessage::MachineQuality,
            );
            mechanic_optional_text_field!(
                ui,
                "Fuel",
                mechanic.fuel.as_deref().unwrap_or_default(),
                id,
                messages,
                MechanicMessage::Recipe,
                RecipeMessage::Fuel,
            );
        }
        Mechanic::Mining(mechanic) => {
            mechanic_text_field!(
                ui,
                "Resource",
                &mechanic.resource,
                id,
                messages,
                MechanicMessage::Mining,
                MiningMessage::Resource,
            );
            mechanic_id_fields!(
                ui,
                &mechanic.machine,
                "Machine",
                id,
                messages,
                MechanicMessage::Mining,
                MiningMessage::MachineId,
                MiningMessage::MachineQuality,
            );
        }
        Mechanic::Spoil(mechanic) => mechanic_id_fields!(
            ui,
            &mechanic.item,
            "Item",
            id,
            messages,
            MechanicMessage::Spoil,
            SpoilMessage::ItemId,
            SpoilMessage::ItemQuality,
        ),
        Mechanic::Plant(mechanic) => mechanic_id_fields!(
            ui,
            &mechanic.seed,
            "Seed",
            id,
            messages,
            MechanicMessage::Plant,
            PlantMessage::SeedId,
            PlantMessage::SeedQuality,
        ),
        Mechanic::ItemFuel(mechanic) => mechanic_id_fields!(
            ui,
            &mechanic.item,
            "Item",
            id,
            messages,
            MechanicMessage::ItemFuel,
            ItemFuelMessage::ItemId,
            ItemFuelMessage::ItemQuality,
        ),
        Mechanic::ItemLaunch(mechanic) => {
            mechanic_id_fields!(
                ui,
                &mechanic.item,
                "Item",
                id,
                messages,
                MechanicMessage::ItemLaunch,
                ItemLaunchMessage::ItemId,
                ItemLaunchMessage::ItemQuality,
            );
            let mut weight_mode = mechanic.weight_mode;
            if ui
                .checkbox(&mut weight_mode, "Weight-limited launch")
                .changed()
            {
                send(
                    messages,
                    id,
                    MechanicMessage::ItemLaunch(ItemLaunchMessage::WeightMode(weight_mode)),
                );
            }
        }
        Mechanic::Generator(mechanic) => {
            mechanic_id_fields!(
                ui,
                &mechanic.generator,
                "Generator",
                id,
                messages,
                MechanicMessage::Generator,
                GeneratorMessage::GeneratorId,
                GeneratorMessage::GeneratorQuality,
            );
            mechanic_text_field!(
                ui,
                "Fluid",
                &mechanic.fluid,
                id,
                messages,
                MechanicMessage::Generator,
                GeneratorMessage::Fluid,
            );
        }
        Mechanic::Boiler(mechanic) => {
            mechanic_id_fields!(
                ui,
                &mechanic.boiler,
                "Boiler",
                id,
                messages,
                MechanicMessage::Boiler,
                BoilerMessage::BoilerId,
                BoilerMessage::BoilerQuality,
            );
            mechanic_text_field!(
                ui,
                "Fluid",
                &mechanic.fluid,
                id,
                messages,
                MechanicMessage::Boiler,
                BoilerMessage::Fluid,
            );
            mechanic_optional_text_field!(
                ui,
                "Fuel",
                mechanic.fuel.as_deref().unwrap_or_default(),
                id,
                messages,
                MechanicMessage::Boiler,
                BoilerMessage::Fuel,
            );
        }
        Mechanic::Reactor(mechanic) => {
            mechanic_id_fields!(
                ui,
                &mechanic.reactor,
                "Reactor",
                id,
                messages,
                MechanicMessage::Reactor,
                ReactorMessage::ReactorId,
                ReactorMessage::ReactorQuality,
            );
            let mut neighbours = mechanic.neighbours;
            if ui
                .add(egui::DragValue::new(&mut neighbours).range(0..=8))
                .changed()
            {
                send(
                    messages,
                    id,
                    MechanicMessage::Reactor(ReactorMessage::Neighbours(neighbours)),
                );
            }
            mechanic_optional_text_field!(
                ui,
                "Fuel",
                mechanic.fuel.as_deref().unwrap_or_default(),
                id,
                messages,
                MechanicMessage::Reactor,
                ReactorMessage::Fuel,
            );
        }
        _ => {
            ui.label("Unsupported mechanic variant");
        }
    }
}

fn id_fields(
    ui: &mut egui::Ui,
    value: &IdWithQuality,
    label: &str,
    id: MechanicId,
    make_id: impl Fn(String) -> MechanicMessage,
    make_quality: impl Fn(String) -> MechanicMessage,
    messages: &mut Vec<AppMessage>,
) {
    text_field(ui, label, &value.id, id, make_id, messages);
    text_field(
        ui,
        &format!("{label} quality"),
        &value.quality,
        id,
        make_quality,
        messages,
    );
}

fn text_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    id: MechanicId,
    make: impl Fn(String) -> MechanicMessage,
    messages: &mut Vec<AppMessage>,
) {
    let mut draft = value.to_string();
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.text_edit_singleline(&mut draft).changed() {
            send(messages, id, make(draft));
        }
    });
}

fn optional_text_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    id: MechanicId,
    make: impl Fn(String) -> MechanicMessage,
    messages: &mut Vec<AppMessage>,
) {
    text_field(ui, label, value, id, make, messages);
    ui.small("Leave empty to clear");
}

fn send(messages: &mut Vec<AppMessage>, id: MechanicId, message: MechanicMessage) {
    messages.push(AppMessage::Mechanic { id, message });
}
