use egui::Sense;

use crate::factorio::{
    common::ReverseOrderInfo, editor::icon::Icon, model::context::FactorioContext,
};
/// 这个就不按 Group 分成多个 Tab 了，全部排列出来
pub struct ReverseItemSelector<'a> {
    pub ctx: &'a FactorioContext,
    pub type_name: &'a String,
    pub reverse_order: &'a ReverseOrderInfo,
    pub filter: Box<dyn Fn(&'a String, &'a FactorioContext) -> bool>,
    pub selected_item: &'a mut Option<String>,
}

impl egui::Widget for ReverseItemSelector<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let mut response = ui.response().clone();
        let available_space = ui.available_size();
        let group_count = (available_space.x as usize / 70).max(4);
        let item_count = (available_space.x as usize / 35).max(8);
        let id = ui.id();

        egui::Grid::new("ReverseItemGroupGrid")
            .min_row_height(35.0)
            .min_col_width(35.0)
            .max_col_width(35.0)
            .spacing(egui::Vec2 { x: 0.0, y: 0.0 })
            .show(ui, |ui| {
                let mut item_index = 0;
                let mut last_order: Option<(usize, usize, usize)> = None;
                for (name, order) in self.reverse_order.iter() {
                    if (item_index % item_count) == 0 && item_index != 0 {
                        ui.end_row();
                    }
                    if let Some(last) = last_order {
                        if order.0 != last.0 || order.1 != last.1 {
                            // 换行
                            ui.end_row();
                        }
                        last_order = Some(order.clone());
                    }
                    if !(self.filter)(&name, self.ctx) {
                        continue;
                    }
                    if ui
                        .add(Icon {
                            ctx: self.ctx,
                            type_name: self.type_name,
                            item_name: &name,
                            size: 32.0,
                            quality: 0,
                        })
                        .interact(Sense::click())
                        .clicked()
                    {
                        self.selected_item.replace(name.clone());
                    }
                    item_index += 1;
                }
            });

        response
    }
}
