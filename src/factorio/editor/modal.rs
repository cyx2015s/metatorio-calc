use egui::ModalResponse;

use crate::factorio::{DataContext, IdWithQuality, selector::Selector};

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

pub struct SelectorModal<'a, Input, Output>
where
    Input: 'a + ?Sized,
{
    label_str: &'a str,
    id: egui::Id,
    toggle: bool,
    selector: Option<Selector<'a, Input, Output>>,
}

impl<'a, Input, Output> SelectorModal<'a, Input, Output>
where
    Input: 'a + ?Sized,
{
    pub fn new(id: egui::Id, label_str: &'a str) -> Self {
        Self {
            id,
            label_str,
            toggle: false,
            selector: None,
        }
    }

    pub fn with_toggle(mut self, toggle: bool) -> Self {
        self.toggle = toggle;
        self
    }

    pub fn with_selector(mut self, selector: Selector<'a, Input, Output>) -> Self {
        self.selector = Some(selector);
        self
    }
}

pub fn str_filter(s: &str, f: &DataContext, type_name: &str, filter_string: &str) -> bool {
    s.to_lowercase().contains(&filter_string.to_lowercase())
        || f.get_display_name(type_name, s)
            .to_lowercase()
            .contains(&filter_string.to_lowercase())
}

#[derive(Debug, Clone, Default)]
pub struct FilterString(pub String);

impl egui::Widget for SelectorModal<'_, str, String> {
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        assert!(self.selector.is_some(), "无法选中");
        let mut widget = self.selector.take().unwrap();
        let mut response = ui.response().clone();
        show_modal(self.id, self.toggle, ui, |ui| {
            let mut filter_string = ui
                .memory(move |mem| {
                    mem.data
                        .get_temp::<FilterString>(self.id)
                        .unwrap_or_default()
                })
                .0;
            ui.label(self.label_str);
            ui.add(egui::widgets::TextEdit::singleline(&mut filter_string).hint_text("筛选器……"));
            ui.memory_mut(|mem| {
                mem.data
                    .insert_temp(self.id, FilterString(filter_string.clone()));
            });
            let type_name = widget.type_name;
            widget = widget.chain_filter(move |s, f| str_filter(s, f, type_name, &filter_string));

            egui::ScrollArea::vertical()
                .max_width(f32::INFINITY)
                .auto_shrink(false)
                .show(ui, |ui| {
                    response = response.union(ui.add(widget));
                });

            if response.should_close() {
                ui.close();
            }
        });
        response
    }
}

impl egui::Widget for SelectorModal<'_, IdWithQuality, IdWithQuality> {
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        assert!(self.selector.is_some(), "无法选中");
        let mut response = ui.response().clone();
        let mut widget = self.selector.take().unwrap();
        show_modal(self.id, self.toggle, ui, |ui| {
            let mut filter_string = ui
                .memory(move |mem| {
                    mem.data
                        .get_temp::<FilterString>(self.id)
                        .unwrap_or_default()
                })
                .0;
            ui.label(self.label_str);
            ui.add(egui::widgets::TextEdit::singleline(&mut filter_string).hint_text("筛选器……"));
            ui.memory_mut(|mem| {
                mem.data
                    .insert_temp(self.id, FilterString(filter_string.clone()));
            });
            let type_name = widget.type_name;
            widget = widget
                .chain_filter(move |s, f| str_filter(&s.0, f, type_name, &filter_string))
                .with_forget(self.toggle);

            egui::ScrollArea::vertical()
                .max_width(f32::INFINITY)
                .auto_shrink(false)
                .show(ui, |ui| {
                    response = response.union(ui.add(widget));
                });

            if response.should_close() {
                ui.close();
            }
        });
        response
    }
}
