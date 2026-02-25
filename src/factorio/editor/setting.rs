use std::collections::{HashSet, VecDeque};

use egui::DragValue;

use crate::factorio::{
    DataContext, ProjectContext, TimeScale, icon::Icon, modal::SelectorModal, selector::Selector,
    update_accessibles,
};

#[derive(Debug)]
pub struct UserContextEditor<'a> {
    pub data: &'a DataContext,
    pub proj: &'a mut ProjectContext,
}

impl<'a> UserContextEditor<'a> {
    pub fn new(data: &'a DataContext, proj: &'a mut ProjectContext) -> Self {
        Self { data, proj }
    }
}

impl egui::Widget for UserContextEditor<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let response = ui.response();
        ui.set_min_width(ui.available_width());
        ui.set_min_height(ui.available_height());
        ui.heading("时间尺度");
        egui::ComboBox::new("time-scale", "时间尺度")
            .selected_text(match self.proj.time_scale {
                TimeScale::Hours => "小时",
                TimeScale::Minutes => "分钟",
                TimeScale::Seconds => "秒",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.proj.time_scale, TimeScale::Hours, "小时");
                ui.selectable_value(&mut self.proj.time_scale, TimeScale::Minutes, "分钟");
                ui.selectable_value(&mut self.proj.time_scale, TimeScale::Seconds, "秒");
            });
        ui.heading("科技里程碑");
        ui.checkbox(&mut self.proj.all_accessible, "选择物品时无视里程碑限制");
        let icon = ui.add(Icon::new(self.data, "entity", "entity-unknown"));

        let mut new_tech_milestone = None;

        let mut recalc_accessible = false;
        ui.add(
            SelectorModal::new(icon.id, "选择科技")
                .with_selector(
                    Selector::new(self.data, "technology").with_output(&mut new_tech_milestone),
                )
                .with_toggle(icon.clicked()),
        );
        if let Some(tech_name) = new_tech_milestone {
            self.proj.tech_milestones.push((tech_name, true));
            recalc_accessible = true;
        }
        let mut recursively_unlock = None;
        self.proj.tech_milestones.retain_mut(|(name, unlocked)| {
            let mut selected_tech: Option<String> = None;

            let mut deleted = false;
            ui.horizontal(|ui| {
                let icon = ui.add(Icon::new(self.data, "technology", name));
                ui.label(self.data.get_display_name("technology", name));
                ui.add(SelectorModal::new(icon.id, "选择科技").with_selector(
                    Selector::new(self.data, "technology").with_output(&mut selected_tech),
                ));
                if ui.checkbox(unlocked, "解锁").changed() {
                    if *unlocked {
                        // 切换为解锁时，遍历前置科技并解锁
                        recursively_unlock = Some(name.clone());
                    }
                    recalc_accessible = true;
                }
                if ui.button("删除").clicked() {
                    recalc_accessible = true;
                    deleted = true;
                }
            });
            if let Some(new_tech_name) = selected_tech {
                *name = new_tech_name;
                recalc_accessible = true;
            }
            !deleted
        });
        if let Some(unlocked_tech) = recursively_unlock {
            let mut queue = VecDeque::new();
            let mut visited = HashSet::new();
            queue.push_back(unlocked_tech);
            while let Some(tech_name) = queue.pop_front() {
                if visited.contains(&tech_name) {
                    continue;
                }
                visited.insert(tech_name.clone());
                if let Some(tech) = self.data.technologies.get(&tech_name) {
                    for prereq in &tech.prerequisites {
                        if let Some((_, unlocked)) = self
                            .proj
                            .tech_milestones
                            .iter_mut()
                            .find(|(name, _)| name == prereq)
                            && !*unlocked
                        {
                            *unlocked = true;
                        }

                        queue.push_back(prereq.clone());
                    }
                }
            }
        }
        if recalc_accessible {
            update_accessibles(self.proj, self.data);
            for (tech, unlocked) in self.proj.tech_milestones.iter_mut() {
                *unlocked = self.proj.accessible_technologies.contains(tech);
            }
        }

        let button = ui.button("查看解锁的配方");
        ui.add(
            SelectorModal::new(button.id, "已解锁的配方")
                .with_toggle(button.clicked())
                .with_selector(
                    Selector::new(self.data, "recipe").with_filter(|s: &str, _f| {
                        self.proj.accessible_prototypes["recipe"].contains_key(s)
                    }),
                ),
        );
        let button = ui.button("查看解锁的实体");
        ui.add(
            SelectorModal::new(button.id, "已解锁的实体")
                .with_toggle(button.clicked())
                .with_selector(
                    Selector::new(self.data, "entity").with_filter(|s: &str, _| {
                        !self.proj.accessible_prototypes.contains_key("entity")
                            || self.proj.accessible_prototypes["entity"].contains_key(s)
                    }),
                ),
        );
        ui.separator();
        ui.heading("采矿产能");
        let mut mining_productivity = (self.proj.mining_productivity * 100.0) as i32;
        ui.add(
            DragValue::new(&mut mining_productivity)
                .suffix("%")
                .speed(1)
                .range(0..=100000),
        );
        self.proj.mining_productivity = mining_productivity as f64 / 100.0;
        ui.heading("配方产能");

        for (recipe_name, productivity) in self.proj.recipe_productivity.iter_mut() {
            let mut value = (*productivity * 100.0) as i32;
            ui.horizontal(|ui| {
                ui.add(Icon::new(self.data, "recipe", recipe_name));
                ui.label(self.data.get_display_name("recipe", recipe_name));
                ui.add(
                    DragValue::new(&mut value)
                        .suffix("%")
                        .speed(1)
                        .range(0..=32770),
                );
            });
            *productivity = value as f64 / 100.0;
        }
        response
    }
}
