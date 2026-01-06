use eframe::*;
use egui::{
    Id, Vec2,
    epaint::text::{FontInsert, InsertFontFamily},
};

use crate::concept::GameContextCreatorView;

pub mod concept;
pub mod factorio;

pub struct MainPage {
    pub creators: Vec<(String, Box<dyn GameContextCreatorView>)>,
    pub subview_receiver: std::sync::mpsc::Receiver<Box<dyn Subview>>,
    pub subview_sender: std::sync::mpsc::Sender<Box<dyn Subview>>,
    pub subviews: Vec<(String, Box<dyn Subview>)>,
    pub selected: usize,
}

pub trait Subview: Send {
    fn view(&mut self, ui: &mut egui::Ui);
    fn should_close(&self) -> bool {
        false
    }
}

impl MainPage {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        add_font(&cc.egui_ctx);
        let (tx, rx) = std::sync::mpsc::channel();
        let mut ret = Self {
            creators: vec![(
                "预设：加载异星工厂上下文".to_string(),
                Box::new(factorio::view::FactorioContextCreatorView::default()),
            )],
            subview_receiver: rx,
            subview_sender: tx,
            subviews: vec![],
            selected: 0,
        };
        for creator in &mut ret.creators {
            creator.1.set_subview_sender(ret.subview_sender.clone());
        }
        ret
    }
}

impl eframe::App for MainPage {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left(Id::new("LeftPanel"))
            .width_range(200.0..=280.0)
            .show(ctx, |ui| {
                ui.heading("切向量化 Metatorio");
                for (i, creator) in self.creators.iter_mut().enumerate() {
                    if ui
                        .selectable_label(self.selected == i, creator.0.to_string())
                        .clicked()
                    {
                        self.selected = i;
                    }
                }

                while let Ok(subview) = self.subview_receiver.try_recv() {
                    let name = format!("子视图 {}", self.subviews.len() + 1);
                    self.subviews.push((name, subview));
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
                self.creators[self.selected].1.view(ui);
            });
        } else {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.subviews[self.selected - self.creators.len()]
                    .1
                    .view(ui);
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
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_module_path(false)
        .format_target(false)
        .format_file(true)
        .format_line_number(true)
        .init();
    log::info!("应用程序启动");
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
