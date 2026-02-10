use crate::factorio::{
    FactorioContext, TimeScale,
    icon::Icon,
    modal::{SelectorModal, show_modal},
    selector::Selector,
};

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
        ui.heading("时间尺度");
        egui::ComboBox::new("time-scale", "时间尺度")
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
        ui.heading("科技里程碑");
        let icon = ui
            .add(Icon::new(self.game, "entity", "entity-unknown"))
            .interact(egui::Sense::click());
        let mut new_tech_milestone = None;
        ui.add(
            SelectorModal::new(icon.id, self.game, "选择科技")
                .with_selector(
                    Selector::new(self.game, "technology").with_output(&mut new_tech_milestone),
                )
                .with_toggle(icon.clicked()),
        );
        if let Some(tech_name) = new_tech_milestone {
            self.game.user.tech_milestones.push((tech_name, true));
        }
        let mut delete_target = None;
        for idx in 0..self.game.user.tech_milestones.len() {
            let (tech_name, unlocked) = &self.game.user.tech_milestones[idx];

            let mut selected_tech: Option<String> = None;
            let mut new_unlocked = *unlocked;
            ui.horizontal(|ui| {
                let icon = ui
                    .add(Icon::new(self.game, "technology", &tech_name))
                    .interact(egui::Sense::click());
                ui.add(
                    SelectorModal::new(icon.id, self.game, "选择科技").with_selector(
                        Selector::new(self.game, "technology").with_output(&mut selected_tech),
                    ),
                );
                ui.checkbox(&mut new_unlocked, "解锁");
                if ui.button("删除").clicked() {
                    delete_target = Some(idx);
                }
            });
            if let Some(new_tech_name) = selected_tech {
                self.game.user.tech_milestones[idx].0 = new_tech_name;
            }
            self.game.user.tech_milestones[idx].1 = new_unlocked;
        }
        if let Some(idx) = delete_target {
            self.game.user.tech_milestones.remove(idx);
        }
        response
    }
}
