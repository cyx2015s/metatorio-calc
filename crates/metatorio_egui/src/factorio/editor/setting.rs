use std::collections::VecDeque;

use egui::DragValue;

use crate::{
    concept::AIndexSet,
    factorio::{
        DataContext, ProjectContext, TimeScale, icon::Icon, modal::SelectorModal,
        resolve_milestone_graph, selector::Selector, style::card_frame, update_accessibles,
    },
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

        ui.heading(t!("metatorio.time-scale").to_string());
        egui::ComboBox::new("time-scale", t!("metatorio.time-scale"))
            .selected_text(match self.proj.time_scale {
                TimeScale::Hours => t!("metatorio.hours"),
                TimeScale::Minutes => t!("metatorio.minutes"),
                TimeScale::Seconds => t!("metatorio.seconds"),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.proj.time_scale,
                    TimeScale::Hours,
                    t!("metatorio.hours"),
                );
                ui.selectable_value(
                    &mut self.proj.time_scale,
                    TimeScale::Minutes,
                    t!("metatorio.minutes"),
                );
                ui.selectable_value(
                    &mut self.proj.time_scale,
                    TimeScale::Seconds,
                    t!("metatorio.seconds"),
                );
            });
        ui.heading(t!("metatorio.tech-milestones").to_string());
        ui.checkbox(
            &mut self.proj.all_accessible,
            t!("metatorio.ignore-milestone-limit"),
        );
        let icon = ui.add(Icon::new(self.data, "entity", "entity-unknown"));

        let mut new_tech_milestone = None;

        let mut recalc_accessible = false;
        ui.add(
            SelectorModal::new(
                icon.id,
                t!("metatorio.select-technology").to_string().as_str(),
            )
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
        let mut last_depth = 0;
        egui::Grid::new("tech-tree").show(ui, |ui| {
            ui.style_mut().spacing.interact_size.y = 0.0;
            self.proj.tech_milestones.retain_mut(|(name, unlocked)| {
                let mut selected_tech: Option<String> = None;
                if !self.proj.milestone_graph.contains_key(name.as_str()) {
                    return true;
                }
                let cur_depth = self.proj.milestone_graph[name.as_str()].depth;
                if cur_depth != last_depth {
                    ui.end_row();
                }
                last_depth = cur_depth;

                let mut deleted = false;

                card_frame(ui)
                    .fill(if *unlocked {
                        egui::Color32::GREEN
                    } else {
                        egui::Color32::GRAY
                    })
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            let icon = ui
                                .add_sized([48.0, 48.0], Icon::new(self.data, "technology", name));

                            ui.add(
                                SelectorModal::new(
                                    icon.id,
                                    t!("metatorio.select-technology").to_string().as_str(),
                                )
                                .with_selector(
                                    Selector::new(self.data, "technology")
                                        .with_output(&mut selected_tech),
                                ),
                            );
                            ui.vertical(|ui| {
                                ui.set_min_width(48.0);
                                if ui.checkbox(unlocked, t!("metatorio.unlock")).changed() {
                                    if *unlocked {
                                        // 切换为解锁时，遍历前置科技并解锁
                                        recursively_unlock = Some(name.clone());
                                    }
                                    recalc_accessible = true;
                                }
                                if ui.button(t!("metatorio.delete")).clicked() {
                                    recalc_accessible = true;
                                    deleted = true;
                                }
                            });
                        });
                    });
                if let Some(new_tech_name) = selected_tech {
                    *name = new_tech_name;
                    recalc_accessible = true;
                }
                !deleted
            });
        });
        if let Some(unlocked_tech) = recursively_unlock {
            let mut queue = VecDeque::new();
            let mut visited = AIndexSet::default();
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
            self.proj.milestone_graph =
                resolve_milestone_graph(self.data, &self.proj.tech_milestones);
            self.proj.tech_milestones.sort_by_cached_key(|v| {
                (self.proj.milestone_graph[v.0.as_str()].depth, v.0.clone())
            });
        }

        let button = ui.button(t!("metatorio.view-unlocked-recipes"));
        ui.add(
            SelectorModal::new(
                button.id,
                t!("metatorio.unlocked-recipes").to_string().as_str(),
            )
            .with_toggle(button.clicked())
            .with_selector(
                Selector::new(self.data, "recipe").with_filter(|s: &str, _f| {
                    self.proj.accessible_prototypes["recipe"].contains_key(s)
                }),
            ),
        );
        let button = ui.button(t!("metatorio.view-unlocked-entities"));
        ui.add(
            SelectorModal::new(
                button.id,
                t!("metatorio.unlocked-entities").to_string().as_str(),
            )
            .with_toggle(button.clicked())
            .with_selector(
                Selector::new(self.data, "entity").with_filter(|s: &str, _| {
                    !self.proj.accessible_prototypes.contains_key("entity")
                        || self.proj.accessible_prototypes["entity"].contains_key(s)
                }),
            ),
        );
        ui.separator();
        ui.heading(t!("metatorio.mining-productivity").to_string());
        let mut mining_productivity = (self.proj.mining_productivity * 100.0) as i32;
        ui.add(
            DragValue::new(&mut mining_productivity)
                .suffix("%")
                .speed(1)
                .range(0..=100000),
        );
        self.proj.mining_productivity = mining_productivity as f64 / 100.0;
        ui.heading(t!("metatorio.recipe-productivity").to_string());

        ui.checkbox(
            &mut self.proj.ignore_productivity,
            t!("metatorio.ignore-recipe-productivity"),
        );
        egui::Grid::new("recipe-productivity").show(ui, |ui| {
            for (recipe_name, productivity) in self.proj.recipe_productivity.iter_mut() {
                let mut value = (*productivity * 100.0) as i32;

                ui.vertical(|ui| {
                    ui.add_sized([35.0, 35.0], Icon::new(self.data, "recipe", recipe_name));
                    ui.add(
                        DragValue::new(&mut value)
                            .suffix("%")
                            .speed(1)
                            .range(0..=32770),
                    );
                });
                *productivity = value as f64 / 100.0;
                if ui.available_size().x < 40.0 {
                    ui.end_row();
                }
            }
        });
        response
    }
}
