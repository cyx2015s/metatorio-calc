use egui::ModalResponse;

use crate::factorio::{FactorioContext, IdWithQuality, selector::Selector};

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
    factorio: &'a FactorioContext,
    label_str: &'a str,
    id: egui::Id,
    toggle: bool,
    selector: Option<Selector<'a, Input, Output>>,
}

impl<'a, Input, Output> SelectorModal<'a, Input, Output>
where
    Input: 'a + ?Sized,
{
    pub fn new(id: egui::Id, factorio: &'a FactorioContext, label_str: &'a str) -> Self {
        Self {
            id,
            factorio,
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
            widget = widget.chain_filter(move |s, f| {
                s.to_lowercase().contains(&filter_string.to_lowercase())
                    || f.get_display_name(type_name, s)
                        .to_lowercase()
                        .contains(&filter_string.to_lowercase())
            });

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
        if self.factorio.qualities.len() == 1 {
            // 回退到普通选择器
            let mut degenerated: Option<String> = None;

            let old_selector = self.selector.take().unwrap();
            let mut selector = Selector::new(self.factorio, old_selector.type_name);
            if let Some(filter) = old_selector.filter {
                selector = selector.with_filter(move |s: &str, f: &FactorioContext| {
                    let id_with_quality = IdWithQuality(s.to_string(), 0);
                    filter(&id_with_quality, f)
                });
            }
            if let Some(hover) = old_selector.hover {
                selector = selector.with_hover(move |ui, s, factorio| {
                    let id_with_quality = IdWithQuality(s.to_string(), 0);
                    hover(ui, &id_with_quality, factorio);
                });
            }
            if let Some(current) = old_selector.current {
                selector = selector.with_current(&mut current.0);
            }
            selector = selector.with_output(&mut degenerated);
            ui.add(
                SelectorModal::new(self.id.with("degenerated"), self.factorio, self.label_str)
                    .with_selector(selector)
                    .with_toggle(self.toggle),
            );

            if let Some(selected) = degenerated
                && let Some(&mut ref mut output) = old_selector.output
            {
                *output = Some(IdWithQuality(selected, 0));
                // response.mark_changed();
                // response.set_close();
            }

            return response;
        }
        show_modal(self.id, self.toggle, ui, |ui| {
            let mut widget = self.selector.take().unwrap();
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
                .chain_filter(move |s, f| {
                    if filter_string.is_empty() {
                        return true;
                    }
                    s.0.to_lowercase().contains(&filter_string.to_lowercase())
                        || f.get_display_name(type_name, &s.0)
                            .to_lowercase()
                            .contains(&filter_string.to_lowercase())
                })
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
