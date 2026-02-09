use crate::factorio::{FactorioContext, TimeScale};

#[derive(Debug)]
pub struct UserContextEditor<'a> {
    pub game: &'a mut FactorioContext,
}

impl<'a> UserContextEditor<'a> {
    pub fn new(game: &'a mut FactorioContext) -> Self {
        Self { game }
    }
}

impl egui::Widget for UserContextEditor<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let response = ui.response();
        ui.set_min_width(ui.available_width());
        ui.set_min_height(ui.available_height());
        egui::ComboBox::new("time-scale", "时间标度")
            .selected_text(match self.game.user.time_scale {
                TimeScale::Hours => "小时",
                TimeScale::Minutes => "分钟",
                TimeScale::Seconds => "秒",
                _ => "未知",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.game.user.time_scale, TimeScale::Hours, "小时");
                ui.selectable_value(&mut self.game.user.time_scale, TimeScale::Minutes, "分钟");
                ui.selectable_value(&mut self.game.user.time_scale, TimeScale::Seconds, "秒");
            });
        response
    }
}
