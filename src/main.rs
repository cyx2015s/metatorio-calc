use std::{fmt::Display, fs::read_to_string, path::Path};

use eframe::*;
use egui::epaint::text::{FontInsert, InsertFontFamily};
use serde_json::*;

use crate::ctx::factorio::ui::FactorioPlannerView;

pub(crate) mod context;
pub(crate) mod ctx;
pub(crate) mod lp;

#[derive(Default)]
pub(crate) struct MainFrame {
    pub(crate) st: String,
    pub(crate) num: i32,
    pub(crate) planner: FactorioPlannerView,
}

impl MainFrame {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        add_font(&cc.egui_ctx);
        Self {
            st: "Hello World!".to_string(),
            num: 0,
            planner: FactorioPlannerView::default(),
        }
    }
}

impl eframe::App for MainFrame {
    
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        egui::SidePanel::left("side_panel").width_range(200.0..=250.0).show(ctx, |ui| {
            ui.heading("侧边栏 Side Panel");
            if ui.button("增加 Increment").clicked() {
                self.num += 1;
            }
            ui.label(format!("当前数值 Current number: {}", self.num));
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hello from eframe!");
            ui.label(format!("字符串 String data: {}", self.st));
            ui.text_edit_singleline(&mut self.st);
            ui.label(format!("数字 Numeric data: {}", self.num));
        });
        self.planner.update(&ctx, frame);
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
    run_native(
        "Demo App",
        NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(MainFrame::new(_cc)))),
    )
    .unwrap();
}
