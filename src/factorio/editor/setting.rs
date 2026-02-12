use egui::DragValue;

use crate::factorio::{
    FactorioContext, TimeScale, icon::Icon, modal::SelectorModal, selector::Selector,
    update_accessibles,
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

        let mut recalc_accessible = false;
        ui.add(
            SelectorModal::new(icon.id, self.game, "选择科技")
                .with_selector(
                    Selector::new(self.game, "technology").with_output(&mut new_tech_milestone),
                )
                .with_toggle(icon.clicked()),
        );
        if let Some(tech_name) = new_tech_milestone {
            self.game.user.tech_milestones.push((tech_name, true));
            recalc_accessible = true;
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
                ui.label(self.game.data.get_display_name("technology", tech_name));
                ui.add(
                    SelectorModal::new(icon.id, self.game, "选择科技").with_selector(
                        Selector::new(self.game, "technology").with_output(&mut selected_tech),
                    ),
                );
                if ui.checkbox(&mut new_unlocked, "解锁").changed() {
                    recalc_accessible = true;
                }
                if ui.button("删除").clicked() {
                    recalc_accessible = true;
                    delete_target = Some(idx);
                }
            });
            if let Some(new_tech_name) = selected_tech {
                self.game.user.tech_milestones[idx].0 = new_tech_name;
                recalc_accessible = true;
            }
            self.game.user.tech_milestones[idx].1 = new_unlocked;
        }
        if let Some(idx) = delete_target {
            recalc_accessible = true;
            self.game.user.tech_milestones.remove(idx);
        }
        if recalc_accessible {
            let user = &mut self.game.user;
            let data = &self.game.data;
            update_accessibles(user, data);
        }

        let button = ui.button("查看解锁的配方");
        ui.add(
            SelectorModal::new(button.id, self.game, "已解锁的配方")
                .with_toggle(button.clicked())
                .with_selector(Selector::new(self.game, "recipe").with_filter(
                    |s: &str, f: &FactorioContext| {
                        !f.user.accessible_prototypes.contains_key("recipe")
                            || f.data.recipes[s].enabled
                            || f.user.accessible_prototypes["recipe"].contains_key(s)
                    },
                )),
        );
        let button = ui.button("查看解锁的实体");
        ui.add(
            SelectorModal::new(button.id, self.game, "已解锁的实体")
                .with_toggle(button.clicked())
                .with_selector(Selector::new(self.game, "entity").with_filter(
                    |s: &str, f: &FactorioContext| {
                        !f.user.accessible_prototypes.contains_key("entity")
                            || f.user.accessible_prototypes["entity"].contains_key(s)
                    },
                )),
        );
        ui.separator();
        ui.heading("采矿产能");
        let mut mining_productivity = self.game.user.mining_productivity * 100.0;
        ui.add(
            DragValue::new(&mut mining_productivity)
                .suffix("%")
                .speed(1.0),
        );
        self.game.user.mining_productivity = mining_productivity.floor() / 100.0;
        ui.heading("配方产能");

        self.game
            .user
            .recipe_productivity
            .iter_mut()
            .for_each(|(recipe_name, productivity)| {
                ui.horizontal(|ui| {
                    ui.label(self.game.data.get_display_name("recipe", recipe_name));
                    let mut value = *productivity * 100.0;
                    ui.add(DragValue::new(&mut value).suffix("%").speed(1.0));
                    *productivity = value.floor() / 100.0;
                });
            });
        response
    }
}
