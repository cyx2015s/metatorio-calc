use egui::ModalResponse;

pub fn show_modal<R>(
    id: egui::Id,
    toggle: bool,
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<ModalResponse<R>> {
    let modal_id = id.with("modal");
    if toggle {
        ui.memory_mut(|mem| {
            mem.data
                .insert_temp(modal_id, !mem.data.get_temp(modal_id).unwrap_or(false));
        });
    }
    let is_open = ui.memory(|mem| mem.data.get_temp::<bool>(modal_id).unwrap_or(false));
    if is_open {
        let modal = egui::Modal::new(modal_id).show(ui.ctx(), contents);
        if modal.should_close() {
            ui.memory_mut(|mem| {
                mem.data.insert_temp::<bool>(modal_id, false);
            });
        }
        Some(modal)
    } else {
        None
    }
}
