use eframe::*;
use egui::{
    Id, Vec2,
    epaint::text::{FontInsert, InsertFontFamily},
};

use crate::{ctx::GameContextCreatorView, ctx::factorio};

pub(crate) mod ctx;
pub(crate) mod lp;

#[derive(Default)]
pub(crate) struct MainPage {
    pub(crate) creators: Vec<(String, Box<dyn GameContextCreatorView>)>,
    pub(crate) subviews: Vec<(String, Box<dyn SubView>)>,
    pub(crate) selected: usize,
}

trait SubView {
    fn ui(&mut self, ui: &mut egui::Ui);
}

impl MainPage {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        add_font(&cc.egui_ctx);
        // cc.egui_ctx.set_zoom_factor(1.25);
        Self {
            creators: vec![(
                "预设：加载异星工厂上下文".to_string(),
                Box::new(factorio::view::ContextCreatorView::default()),
            )],
            subviews: vec![],
            selected: 0,
        }
    }
}

impl eframe::App for MainPage {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        egui::SidePanel::left(Id::new("LeftPanel")).show(ctx, |ui| {
            ui.heading("Metatorio");
            for (i, creator) in self.creators.iter_mut().enumerate() {
                if ui
                    .selectable_label(self.selected == i, creator.0.to_string())
                    .clicked()
                {
                    self.selected = i;
                }
                if let Some(subview) = creator.1.try_create_subview() {
                    self.subviews.push((creator.0.clone(), subview));
                    self.selected = self.subviews.len() - 1;
                }
            }

            ui.separator();

            for (i, subview) in self.subviews.iter().enumerate() {
                if ui
                    .selectable_label(
                        self.selected == i + self.creators.len(),
                        subview.0.to_string(),
                    )
                    .clicked()
                {
                    self.selected = i + self.creators.len();
                }
            }
        });
        if self.selected < self.creators.len() {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.creators[self.selected].1.ui(ui);
            });
        } else {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.subviews[self.selected - self.creators.len()].1.ui(ui);
            });
        }
    }
}

fn add_font(ctx: &egui::Context) {
    ctx.add_font(FontInsert::new(
        "LXGW",
        egui::FontData::from_static(include_bytes!("../assets/font.ttf")),
        vec![
            InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority: egui::epaint::text::FontPriority::Highest,
            },
            InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority: egui::epaint::text::FontPriority::Highest,
            },
        ],
    ));
}

fn main() {
    env_logger::init();
    run_native(
        "Demo App",
        NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_maximized(true)
                .with_min_inner_size(Vec2 { x: 800.0, y: 600.0 }),
            ..Default::default()
        },
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            cc.egui_ctx.all_styles_mut(|style| {
                style.interaction.tooltip_delay = 0.0;
                style.interaction.tooltip_grace_time = 0.0;
                style.interaction.show_tooltips_only_when_still = false;
            });
            Ok(Box::new(MainPage::new(cc)))
        }),
    )
    .unwrap();
}
