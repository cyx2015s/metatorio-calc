use egui::ModalResponse;

use crate::{
    concept::{ItemIdent, Mechanic, MechanicProvider, MechanicSender},
    factorio::{
        FactorioContext, IdWithQuality,
        selector::{FilterFn, HoverUi, ItemSelector, ItemWithQualitySelector},
        style::card_frame,
    },
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
    id: egui::Id,
    toggle: bool,
    filter: Option<Box<FilterFn<'a>>>,
    current: Option<&'a mut String>,
    output: Option<&'a mut Option<String>>,
    hover: Option<Box<HoverUi<'a>>>,
    changed: Option<&'a mut bool>,
}

impl<'a> ItemSelectorModal<'a> {
    pub fn new(
        id: egui::Id,
        ctx: &'a FactorioContext,
        label_str: &'a str,
        item_type: &'a str,
    ) -> Self {
        Self {
            id,
            ctx,
            label_str,
            item_type,
            filter: None,
            current: None,
            output: None,
            hover: None,
            toggle: false,
            changed: None,
        }
    }
    pub fn with_toggle(mut self, toggle: bool) -> Self {
        self.toggle = toggle;
        self
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

    pub fn with_hover(
        mut self,
        hover: impl Fn(&mut egui::Ui, &str, &FactorioContext) + 'a,
    ) -> Self {
        self.hover = Some(Box::new(hover));
        self
    }

    pub fn notify_change(mut self, changed: &'a mut bool) -> Self {
        self.changed = Some(changed);
        self
    }
}
#[derive(Debug, Clone, Default)]
pub struct FilterString(pub String);

impl egui::Widget for ItemSelectorModal<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        debug_assert!(
            self.output.is_some() || self.current.is_some(),
            "结果不要了吗，还回家吃饭吗？"
        );
        let mut sentinel = None;
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
            if let Some(hover) = self.hover {
                widget = widget.with_hover(hover);
            }
            if let Some(changed) = self.changed {
                widget = widget.notify_change(changed);
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

pub struct ItemWithQualitySelectorModal<'a> {
    ctx: &'a FactorioContext,
    label_str: &'a str,
    item_type: &'a str,
    id: egui::Id,
    toggle: bool,
    filter: Option<Box<FilterFn<'a>>>,
    current: Option<&'a mut IdWithQuality>,
    output: Option<&'a mut Option<IdWithQuality>>,
    hover: Option<Box<HoverUi<'a>>>,
    changed: Option<&'a mut bool>,
}

impl<'a> ItemWithQualitySelectorModal<'a> {
    pub fn new(
        id: egui::Id,
        ctx: &'a FactorioContext,
        label_str: &'a str,
        item_type: &'a str,
    ) -> Self {
        Self {
            id,
            ctx,
            label_str,
            item_type,
            toggle: false,
            filter: None,
            current: None,
            output: None,
            hover: None,
            changed: None,
        }
    }

    pub fn with_toggle(mut self, toggle: bool) -> Self {
        self.toggle = toggle;
        self
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

    pub fn with_hover(
        mut self,
        hover: impl Fn(&mut egui::Ui, &str, &FactorioContext) + 'a,
    ) -> Self {
        self.hover = Some(Box::new(hover));
        self
    }

    pub fn notify_change(mut self, changed: &'a mut bool) -> Self {
        self.changed = Some(changed);
        self
    }
}

impl egui::Widget for ItemWithQualitySelectorModal<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        debug_assert!(
            self.output.is_some() || self.current.is_some(),
            "结果不要了吗，还回家吃饭吗？"
        );
        let mut sentinel = None;
        let mut degenerated: Option<String> = None;
        if self.ctx.qualities.len() == 1 {
            // 回退到普通选择器

            let mut widget =
                ItemSelectorModal::new(self.id, self.ctx, self.label_str, self.item_type)
                    .with_toggle(self.toggle);
            if let Some(custom_filter) = self.filter {
                widget = widget.with_filter(custom_filter);
            }
            if let Some(current) = self.current {
                widget = widget.with_current(&mut current.0);
            }
            if let Some(hover) = self.hover {
                widget = widget.with_hover(hover);
            }
            if self.output.is_some() {
                widget = widget.with_output(&mut degenerated);
            }
            if let Some(changed) = self.changed {
                widget = widget.notify_change(changed);
            }
            let ret = widget.ui(ui);
            if let Some(selected) = degenerated
                && let Some(&mut ref mut output) = self.output {
                    *output = Some(IdWithQuality(selected, 0));
                    
                }
            return ret;
        }
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
                .with_forget(self.toggle);
            widget = widget.with_output(&mut sentinel);

            if let Some(current) = self.current {
                widget = widget.with_current(current);
            }
            if let Some(custom_filter) = self.filter {
                widget = widget.chain_filter(custom_filter);
            }
            if let Some(hover) = self.hover {
                widget = widget.with_hover(hover);
            }
            if let Some(changed) = self.changed {
                widget = widget.notify_change(changed);
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

pub struct HintModal<'a, I: ItemIdent, C: 'static> {
    ctx: &'a C,
    id: egui::Id,
    toggle: bool,
    flow_sender: &'a MechanicSender<I, C>,
    hint_flows: &'a mut Vec<Box<dyn Mechanic<GameContext = C, ItemIdentType = I> + 'static>>,
    editor_sources: &'a [Box<dyn MechanicProvider<ItemIdentType = I, GameContext = C>>],
}

impl<'a, I: ItemIdent, C: 'static> HintModal<'a, I, C> {
    pub fn new(
        id: egui::Id,
        ctx: &'a C,
        flow_sender: &'a MechanicSender<I, C>,
        hint_flows: &'a mut Vec<Box<dyn Mechanic<GameContext = C, ItemIdentType = I> + 'static>>,
        editor_sources: &'a [Box<dyn MechanicProvider<ItemIdentType = I, GameContext = C>>],
    ) -> Self {
        Self {
            id,
            ctx,
            toggle: false,
            flow_sender,
            hint_flows,
            editor_sources,
        }
    }

    pub fn with_update(mut self, update: bool, item: &'a I, amount: f64) -> Self {
        if update {
            self.toggle = true;
            self.hint_flows.clear();
            for source in self.editor_sources {
                self.hint_flows
                    .extend(source.hint_populate(self.ctx, item, amount));
            }
        } else {
            self.toggle = false;
        }
        self
    }
}

impl<'a, I: ItemIdent, C: 'static> egui::Widget for HintModal<'a, I, C> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        show_modal(self.id, self.toggle, ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.set_min_height(384.0);
                ui.label("推荐配方");
                if self.hint_flows.is_empty() {
                    ui.label("无推荐配方");
                } else {
                    for hint_flow in self.hint_flows.iter_mut() {
                        card_frame(ui).show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal(|ui| {
                                hint_flow.editor_view(ui, self.ctx);
                            });
                            if ui.button("添加").clicked() {
                                self.flow_sender.send(hint_flow.clone()).unwrap();
                            }
                        });
                    }
                }
            });
        });

        ui.response().clone()
    }
}
