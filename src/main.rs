use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

// Git 版本信息
include!(concat!(env!("OUT_DIR"), "/git_hash.rs"));

use egui::Id;

use crate::concept::*;

pub mod concept;
pub mod dyn_deserialize;
pub mod factorio;
pub mod solver;

pub struct MainPage {
    pub creators: Vec<(String, Box<dyn GameContextCreatorView>)>,
    pub subview_receiver: std::sync::mpsc::Receiver<Box<dyn Subview>>,
    pub subview_sender: std::sync::mpsc::Sender<Box<dyn Subview>>,
    pub subviews: Vec<Box<dyn Subview>>,
    pub selected: usize,
}

impl MainPage {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        add_font(&cc.egui_ctx);
        let (tx, rx) = std::sync::mpsc::channel();
        let mut ret = Self {
            creators: vec![(
                "异星工厂".to_string(),
                Box::new(factorio::planner::FactorioContextCreatorView::default()),
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
        let mut request_repaint = true;
        ctx.input(|i| {
            if i.viewport().minimized.unwrap_or_default() {
                request_repaint = false;
            }
        });
        if request_repaint {
            ctx.request_repaint_after_secs(0.1);
        }
        egui::SidePanel::left(egui::Id::new("side"))
            .width_range(200.0..=280.0)
            .show(ctx, |ui| {
                ui.heading("切向量化");
                ui.label(format!("构建哈希: {}", GIT_HASH));
                ui.add(egui::Hyperlink::from_label_and_url(
                    "Github 仓库",
                    "https://github.com/cyx2015s/metatorio-calc",
                ));
                for (i, creator) in self.creators.iter_mut().enumerate() {
                    if ui
                        .selectable_label(self.selected == i, creator.0.to_string())
                        .clicked()
                    {
                        self.selected = i;
                    }
                }

                while let Ok(subview) = self.subview_receiver.try_recv() {
                    self.subviews.push(subview);
                }

                ui.separator();

                for (i, subview) in self.subviews.iter().enumerate() {
                    if ui
                        .selectable_label(self.selected == i + self.creators.len(), subview.name())
                        .on_hover_text(subview.description())
                        .clicked()
                    {
                        self.selected = i + self.creators.len();
                    }
                }
                ui.separator();
                let mut show_font_license =
                    ui.memory(|mem| mem.data.get_temp::<bool>(Id::new("font")).unwrap_or(false));
                if ui.checkbox(&mut show_font_license, "字体协议").clicked() {
                    ui.memory_mut(|mem| {
                        mem.data
                            .insert_temp::<bool>(Id::new("font"), !show_font_license);
                    });
                }
                if show_font_license {
                    egui::Window::new("字体协议")
                        .open(&mut show_font_license)
                        .show(ctx, |ui| {
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                ui.label(include_str!("../assets/LICENSE"));
                            });
                        });
                }
                if ui.button("重新加载图标").clicked() {
                    ui.ctx().forget_all_images();
                }
                ui.memory_mut(|mem| {
                    mem.data.insert_temp(Id::new("font"), show_font_license);
                })
            });
        if self.selected < self.creators.len() {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.creators[self.selected].1.view(ui);
            });
        } else {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.subviews[self.selected - self.creators.len()].view(ui);
            });
        }
    }
}

fn add_font(ctx: &egui::Context) {
    ctx.add_font(egui::epaint::text::FontInsert::new(
        "LXGW",
        egui::FontData::from_static(include_bytes!("../assets/font.ttf")),
        vec![
            egui::epaint::text::InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority: egui::epaint::text::FontPriority::Highest,
            },
            egui::epaint::text::InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority: egui::epaint::text::FontPriority::Highest,
            },
        ],
    ));
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_module_path(true)
        .format_target(false)
        .format_file(false)
        .format_line_number(true)
        .init();
    log::info!("应用程序启动");
    let icon_image = image::load_from_memory(include_bytes!("../assets/icon.png")).unwrap();
    eframe::run_native(
        "Demo App",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_maximized(true)
                .with_min_inner_size(egui::Vec2 { x: 800.0, y: 600.0 })
                .with_title("切向量化 [内内内内测版]")
                .with_icon(egui::IconData {
                    rgba: icon_image.to_rgba8().into_raw(),
                    width: icon_image.width(),
                    height: icon_image.height(),
                }),

            renderer: eframe::Renderer::Wgpu,

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
