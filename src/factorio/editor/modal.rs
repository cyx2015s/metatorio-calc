use egui::ModalResponse;

use crate::factorio::{
    FactorioContext, IdWithQuality,
    selector::{ItemSelector, ItemWithQualitySelector},
};

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

pub struct ItemSelectorModal<'a> {
    ctx: &'a FactorioContext,
    label_str: &'a str,
    item_type: &'a str,
    button: &'a egui::Response,
    filter: Option<Box<dyn Fn(&str, &FactorioContext) -> bool + 'a>>,
    current: Option<&'a mut String>,
    output: Option<&'a mut Option<String>>,
}

impl<'a> ItemSelectorModal<'a> {
    pub fn new(
        ctx: &'a FactorioContext,
        label_str: &'a str,
        item_type: &'a str,
        button: &'a egui::Response,
    ) -> Self {
        Self {
            ctx,
            label_str,
            item_type,
            button,
            filter: None,
            current: None,
            output: None,
        }
    }

    pub fn with_filter(mut self, filter: impl Fn(&str, &FactorioContext) -> bool + 'a) -> Self {
        self.filter = Some(Box::new(filter));
        self
    }

    pub fn with_current(mut self, current: &'a mut String) -> Self {
        self.current = Some(current);
        self
    }

    pub fn with_output(mut self, output: &'a mut Option<String>) -> Self {
        self.output = Some(output);
        self
    }
}
#[derive(Debug, Clone, Default)]
pub struct FilterString(pub String);

impl egui::Widget for ItemSelectorModal<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        assert!(
            self.output.is_some() || self.current.is_some(),
            "结果不要了吗，还回家吃饭吗？"
        );
        let id = self.button.id;
        let mut sentinel = None;
        show_modal(id, self.button.clicked(), ui, |ui| {
            let mut filter_string = ui
                .memory(move |mem| mem.data.get_temp::<FilterString>(id).unwrap_or_default())
                .0;
            ui.label(self.label_str);
            ui.add(egui::widgets::TextEdit::singleline(&mut filter_string).hint_text("筛选器……"));
            ui.memory_mut(|mem| {
                mem.data
                    .insert_temp(id, FilterString(filter_string.clone()));
            });
            let mut widget = ItemSelector::new(self.ctx, self.item_type).with_filter(|s, f| {
                if filter_string.is_empty() {
                    return true;
                }
                s.to_lowercase().contains(&filter_string.to_lowercase())
                    || f.get_display_name(self.item_type, s)
                        .to_lowercase()
                        .contains(&filter_string.to_lowercase())
            });
            widget = widget.with_output(&mut sentinel);

            if let Some(current) = self.current {
                widget = widget.with_current(current);
            }
            if let Some(custom_filter) = self.filter {
                widget = widget.chain_filter(custom_filter);
            }
            egui::ScrollArea::vertical()
                .max_width(f32::INFINITY)
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.add(widget);
                });
            dbg!(&sentinel);
            if let Some(selected) = sentinel {
                if let Some(&mut ref mut output) = self.output {
                    *output = Some(selected);
                }
                ui.close();
            }
        });
        ui.response().clone()
    }
}

pub struct ItemWithQualitySelectorModal<'a> {
    ctx: &'a FactorioContext,
    label_str: &'a str,
    item_type: &'a str,
    button: &'a egui::Response,
    filter: Option<Box<dyn Fn(&str, &FactorioContext) -> bool + 'a>>,
    current: Option<&'a mut IdWithQuality>,
    output: Option<&'a mut Option<IdWithQuality>>,
}

impl<'a> ItemWithQualitySelectorModal<'a> {
    pub fn new(
        ctx: &'a FactorioContext,
        label_str: &'a str,
        item_type: &'a str,
        button: &'a egui::Response,
    ) -> Self {
        Self {
            ctx,
            label_str,
            item_type,
            button,
            filter: None,
            current: None,
            output: None,
        }
    }

    pub fn with_filter(mut self, filter: impl Fn(&str, &FactorioContext) -> bool + 'a) -> Self {
        self.filter = Some(Box::new(filter));
        self
    }

    pub fn with_current(mut self, current: &'a mut IdWithQuality) -> Self {
        self.current = Some(current);
        self
    }

    pub fn with_output(mut self, output: &'a mut Option<IdWithQuality>) -> Self {
        self.output = Some(output);
        self
    }
}

impl egui::Widget for ItemWithQualitySelectorModal<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        assert!(
            self.output.is_some() || self.current.is_some(),
            "结果不要了吗，还回家吃饭吗？"
        );
        let id = self.button.id;
        let mut sentinel = None;
        let modal = show_modal(id, self.button.clicked(), ui, |ui| {
            let mut filter_string = ui
                .memory(move |mem| mem.data.get_temp::<FilterString>(id).unwrap_or_default())
                .0;
            ui.label(self.label_str);
            ui.add(egui::widgets::TextEdit::singleline(&mut filter_string).hint_text("筛选器……"));
            ui.memory_mut(|mem| {
                mem.data
                    .insert_temp(id, FilterString(filter_string.clone()));
            });
            let mut widget = ItemWithQualitySelector::new(self.ctx, self.item_type)
                .with_filter(|s, f| {
                    if filter_string.is_empty() {
                        return true;
                    }
                    s.to_lowercase().contains(&filter_string.to_lowercase())
                        || f.get_display_name(self.item_type, s)
                            .to_lowercase()
                            .contains(&filter_string.to_lowercase())
                })
                .with_forget(self.button.clicked());
            widget = widget.with_output(&mut sentinel);

            if let Some(current) = self.current {
                widget = widget.with_current(current);
            }
            if let Some(custom_filter) = self.filter {
                widget = widget.chain_filter(custom_filter);
            }
            egui::ScrollArea::vertical()
                .max_width(f32::INFINITY)
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.add(widget);
                });
            if let Some(selected) = sentinel {
                if let Some(&mut ref mut output) = self.output {
                    *output = Some(selected);
                }
                ui.close();
            }
        });
        ui.response().clone()
    }
}
