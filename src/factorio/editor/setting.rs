use crate::factorio::{
    FactorioContext, Modifier, RecipeResult, TimeScale, icon::Icon, modal::SelectorModal,
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
            self.game.user.accessible_technologies = crate::factorio::user::resolve_dependency(
                &self.game.data.technologies,
                &self.game.user.tech_milestones,
            );
            self.game.user.accessible_prototypes.clear();
            for tech_name in &self.game.user.accessible_technologies {
                if let Some(tech) = self.game.data.technologies.get(tech_name) {
                    for modifier in &tech.effects {
                        match modifier {
                            Modifier::UnlockRecipe { recipe } => {
                                if let Some(recipe_proto) = self.game.data.recipes.get(recipe) {
                                    self.game
                                        .user
                                        .accessible_prototypes
                                        .entry("recipe".to_string())
                                        .or_default()
                                        .insert(recipe.clone(), true);
                                    for result in &recipe_proto.results {
                                        match result {
                                            RecipeResult::Item(item) => {
                                                self.game
                                                    .user
                                                    .accessible_prototypes
                                                    .entry("item".to_string())
                                                    .or_default()
                                                    .insert(item.name.clone(), true);
                                            }
                                            RecipeResult::Fluid(fluid) => {
                                                self.game
                                                    .user
                                                    .accessible_prototypes
                                                    .entry("fluid".to_string())
                                                    .or_default()
                                                    .insert(fluid.name.clone(), true);
                                            }
                                        }
                                    }
                                }
                            }
                            Modifier::UnlockSpaceLocation { space_location } => {
                                self.game
                                    .user
                                    .accessible_prototypes
                                    .entry("space-location".to_string())
                                    .or_default()
                                    .insert(space_location.clone(), true);
                            }
                            Modifier::UnlockQuality { quality } => {
                                self.game
                                    .user
                                    .accessible_prototypes
                                    .entry("quality".to_string())
                                    .or_default()
                                    .insert(quality.clone(), true);
                            }
                            _ => {}
                        }
                    }
                }
            }

            for (_, resource) in &self.game.data.resources {
                if let Some(mining) = resource.base.minable.as_ref() {
                    for result in &mining.results {
                        match result {
                            RecipeResult::Item(item) => {
                                self.game
                                    .user
                                    .accessible_prototypes
                                    .entry("item".to_string())
                                    .or_default()
                                    .insert(item.name.clone(), true);
                            }
                            RecipeResult::Fluid(fluid) => {
                                self.game
                                    .user
                                    .accessible_prototypes
                                    .entry("fluid".to_string())
                                    .or_default()
                                    .insert(fluid.name.clone(), true);
                            }
                        }
                    }
                    if let Some(result) = &mining.result {
                        self.game
                            .user
                            .accessible_prototypes
                            .entry("item".to_string())
                            .or_default()
                            .insert(result.clone(), true);
                    }
                }
            }

            for (item_name, item) in &self.game.data.items {
                if let Some(place_result) = &item.place_result {
                    if self
                        .game
                        .user
                        .accessible_prototypes
                        .get("item")
                        .map_or(false, |items| items.contains_key(item_name))
                    {
                        self.game
                            .user
                            .accessible_prototypes
                            .entry("entity".to_string())
                            .or_default()
                            .insert(place_result.clone(), true);
                    }
                }
            }
        }
        ui.label(format!(
            "解锁的科技有: {:?}",
            &self.game.user.accessible_technologies,
        ));
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

        response
    }
}
