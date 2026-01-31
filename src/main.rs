use mimalloc::MiMalloc;

use crate::update::*;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

// Git 版本信息
include!(concat!(env!("OUT_DIR"), "/git_hash.rs"));

pub mod concept;
pub mod dyn_serde;
pub mod error;
pub mod factorio;
pub mod solver;
pub mod toast;
pub mod update;

pub struct MainPage {
    pub creators: Vec<(String, Box<dyn concept::GameContextCreatorView>)>,
    pub subviews: Vec<Box<dyn concept::Subview>>,
    pub selected: usize,

    pub subview_receiver: std::sync::mpsc::Receiver<Box<dyn concept::Subview>>,
    pub subview_sender: std::sync::mpsc::Sender<Box<dyn concept::Subview>>,

    pub exp_cpu_usage: f32,

    pub suitable_release: Result<self_update::update::Release, error::AppError>,
    pub response_receiver:
        std::sync::mpsc::Receiver<Result<self_update::update::Release, error::AppError>>,
    pub request_sender: std::sync::mpsc::Sender<NetworkRequest>,
}

pub enum NetworkRequest {
    FetchReleases,
    SelfUpdate,
}

impl Default for MainPage {
    fn default() -> Self {
        let (subview_sender, subview_receiver) = std::sync::mpsc::channel();
        let (network_response_tx, network_response_rx) = std::sync::mpsc::channel();
        let (network_request_tx, network_request_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || -> Result<(), error::AppError> {
            log::info!("网络线程已启动");
            while let Ok(request) = network_request_rx.recv() {
                let update_downloader = create_update_downloader()?;
                match request {
                    NetworkRequest::FetchReleases => {
                        let release = self_update::update::ReleaseUpdate::get_latest_release(
                            &update_downloader,
                        );
                        match release {
                            Ok(release) => {
                                if get_download_progress() != DownloadProgress::Pending {
                                    if get_download_progress() == DownloadProgress::Completed {
                                        network_response_tx
                                            .send(Err(error::AppError::RestartRequired))?;
                                    }
                                    log::warn!("已有更新正在进行中，忽略新的更新请求");
                                    continue;
                                }

                                if release.version != self_update::cargo_crate_version!() {
                                    log::info!("获取到最新版本: {}", release.version);
                                    network_response_tx.send(Ok(release)).unwrap();
                                } else {
                                    log::info!("当前已是最新版本");
                                    network_response_tx.send(Err(error::AppError::UpToDate))?;
                                }
                            }
                            Err(err) => {
                                log::error!("获取最新版本失败: {:?}", err);
                                network_response_tx.send(Err(error::AppError::Update(format!(
                                    "获取最新版本失败: {:?}",
                                    err
                                ))))?;
                            }
                        }
                    }
                    NetworkRequest::SelfUpdate => {
                        if get_download_progress() != DownloadProgress::Pending {
                            log::warn!("已有更新正在进行中，忽略新的更新请求");
                            continue;
                        }
                        set_download_progress(DownloadProgress::InProgress(0, 0));
                        std::thread::spawn(|| update::update().unwrap());
                    }
                }
            }
            log::info!("网络线程已退出");
            Ok(())
        });
        Self {
            creators: vec![],
            subview_receiver,
            subview_sender,
            selected: 0,
            subviews: vec![],
            exp_cpu_usage: 0.0,
            suitable_release: Err(error::AppError::None),
            request_sender: network_request_tx,
            response_receiver: network_response_rx,
        }
    }
}

impl MainPage {
    pub fn add_creator(
        &mut self,
        name: &str,
        mut creator: Box<dyn concept::GameContextCreatorView>,
    ) {
        creator.set_subview_sender(self.subview_sender.clone());
        self.creators.push((name.to_string(), creator));
    }
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        add_font(&cc.egui_ctx);
        let mut ret = Self {
            creators: vec![(
                "异星工厂".to_string(),
                Box::new(factorio::planner::FactorioContextCreatorView::default()),
            )],
            ..Default::default()
        };
        for creator in &mut ret.creators {
            creator.1.set_subview_sender(ret.subview_sender.clone());
        }
        ret
    }
}

impl eframe::App for MainPage {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        let mut request_repaint = true;
        ctx.input(|i| {
            if i.viewport().minimized.unwrap_or_default() {
                request_repaint = false;
            }
        });
        if request_repaint {
            ctx.request_repaint_after_secs(0.1);
        }
        let cpu_usage = frame.info().cpu_usage.unwrap_or(0.0);
        self.exp_cpu_usage = self.exp_cpu_usage * 31.0 / 32.0 + cpu_usage / 32.0;
        egui::SidePanel::left(egui::Id::new("side"))
            .width_range(200.0..=280.0)
            .show(ctx, |ui| {
                ui.heading("切向量化");
                ui.label(format!("[构建] Git 哈希: {}", GIT_HASH));
                ui.label(format!(
                    "[性能] 帧生成时间: {:.2}ms",
                    self.exp_cpu_usage * 1000.0
                ));
                ui.separator();
                ui.label(format!("当前版本: {}", self_update::cargo_crate_version!()));
                if ui.button("检查更新").clicked() {
                    self.request_sender
                        .send(NetworkRequest::FetchReleases)
                        .unwrap();
                }
                let response = self.response_receiver.try_recv();
                match response {
                    Ok(response) => {
                        self.suitable_release = response;
                        match self.suitable_release {
                            Ok(_) => {

                            }
                            Err(ref err) => match err {
                                error::AppError::UpToDate => {
                                    toast::success("当前已是最新版本。");
                                }
                                error::AppError::None => {
                                    
                                }
                                err => {
                                    toast::error(format!("更新检查失败: {:?}", err));
                                }
                            },
                        }
                    }
                    _ => {}
                }
                match &mut self.suitable_release {
                    Ok(release) => {
                        ui.label(format!("可更新新版本: {}", release.version));
                        if ui.button("更新").clicked() {
                            self.request_sender
                                .send(NetworkRequest::SelfUpdate)
                                .unwrap();
                        }
                    }
                    Err(err) => match err {
                        error::AppError::None => {}
                        error::AppError::UpToDate => {
                            ui.label("当前已是最新版本。");
                        }
                        error::AppError::RestartRequired => {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                "更新已下载完成，请重启应用以应用更新。",
                            );
                            if ui.button("重启应用").clicked() {
                                std::process::Command::new(std::env::current_exe().unwrap())
                                    .spawn()
                                    .unwrap();
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        _err => {
                            ui.colored_label(
                                egui::Color32::RED,
                                format!("更新检查失败"),
                            );
                        }
                    },
                }
                ui.add(egui::Hyperlink::from_label_and_url(
                    "Github 仓库",
                    "https://github.com/cyx2015s/metatorio-calc",
                ));
                ui.separator();
                self.creators
                    .iter_mut()
                    .enumerate()
                    .for_each(|(i, creator)| {
                        if ui
                            .selectable_label(self.selected == i, &creator.0)
                            .on_hover_text_at_pointer("点击以选择该游戏环境，右键显示额外菜单")
                            .clicked()
                        {
                            self.selected = i;
                        }
                    });

                while let Ok(subview) = self.subview_receiver.try_recv() {
                    self.subviews.push(subview);
                }

                ui.separator();
                let mut idx = 0;
                self.subviews.retain_mut(|subview| {
                    let label = ui
                        .selectable_label(
                            self.selected == idx + self.creators.len(),
                            subview.name(),
                        )
                        .on_hover_text_at_pointer(subview.description());
                    if label.clicked() {
                        self.selected = idx + self.creators.len();
                    }
                    idx += 1;
                    let mut deleted = false;
                    label.context_menu(|ui| {
                        if ui.button("关闭").clicked() {
                            deleted = true;
                        }
                    });

                    !deleted
                });
                if self.selected >= self.creators.len() + self.subviews.len() {
                    self.selected = 0;
                }
                ui.separator();
                let mut show_font_license = ui.memory(|mem| {
                    mem.data
                        .get_temp::<bool>(egui::Id::new("font"))
                        .unwrap_or(false)
                });
                if ui.checkbox(&mut show_font_license, "字体协议").clicked() {
                    ui.memory_mut(|mem| {
                        mem.data
                            .insert_temp::<bool>(egui::Id::new("font"), !show_font_license);
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
                    mem.data
                        .insert_temp(egui::Id::new("font"), show_font_license);
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
        toast::TOASTS.lock().unwrap().show(ctx);
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
                style.interaction.tooltip_delay = 0.2;
                style.interaction.tooltip_grace_time = 1.0;
                style.interaction.show_tooltips_only_when_still = false;
            });
            Ok(Box::new(MainPage::new(cc)))
        }),
    )
    .unwrap();
}
