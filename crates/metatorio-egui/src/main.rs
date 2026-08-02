// #![cfg_attr(all(not(test), not(debug_assertions)), windows_subsystem = "windows")]

use std::sync::{LazyLock, mpsc::*};

use fust_i18n::update_i18n_ini;
use mimalloc::MiMalloc;

use crate::update::*;
#[macro_use]
extern crate fust_i18n;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

// Git 版本信息
include!(concat!(env!("OUT_DIR"), "/git_hash.rs"));

pub mod concept;

pub mod error;
pub mod factorio;
pub mod math;
pub mod toast;
pub mod update;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectedSubview {
    Title,
    Creator(usize),
    Planner(usize),
    Logs,
}

pub struct MainPage {
    pub creators: Vec<Box<dyn concept::GameContextCreatorView>>,
    pub planners: Vec<Box<dyn concept::SubView>>,
    pub selected: SelectedSubview,

    pub subview_receiver: Receiver<Box<dyn concept::SubView>>,
    pub subview_sender: Sender<Box<dyn concept::SubView>>,

    pub exp_cpu_usage: f32,

    pub suitable_release: Result<self_update::update::Release, error::AppError>,
    pub response_receiver: Receiver<Result<self_update::update::Release, error::AppError>>,
    pub request_sender: Sender<NetworkRequest>,

    pub font_filter: String,
}

pub enum NetworkRequest {
    FetchReleases,
    SelfUpdate,
}

impl Default for MainPage {
    fn default() -> Self {
        let (subview_sender, subview_receiver) = channel();
        let (network_response_tx, network_response_rx) = channel();
        let (network_request_tx, network_request_rx) = channel();
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
                                network_response_tx
                                    .send(Err(error::AppError::Update(err.to_string())))?;
                            }
                        }
                    }
                    NetworkRequest::SelfUpdate => {
                        if get_download_progress() != DownloadProgress::Pending {
                            log::warn!("已有更新正在进行中，忽略新的更新请求");
                            continue;
                        }
                        set_download_progress(DownloadProgress::InProgress(0, 0));
                        let thread_response_tx = network_response_tx.clone();
                        std::thread::spawn(move || {
                            update::update().unwrap();
                            thread_response_tx
                                .send(Err(error::AppError::RestartRequired))
                                .unwrap();
                        });
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
            selected: SelectedSubview::Title,
            planners: vec![],
            exp_cpu_usage: 0.0,
            suitable_release: Err(error::AppError::None),
            request_sender: network_request_tx,
            response_receiver: network_response_rx,
            font_filter: String::new(),
        }
    }
}

impl MainPage {
    pub fn add_creator(&mut self, mut creator: Box<dyn concept::GameContextCreatorView>) {
        creator.set_subview_sender(self.subview_sender.clone());
        self.creators.push(creator);
    }
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut ret = Self {
            creators: vec![Box::new(factorio::planner::ContextCreatorView::default())],
            ..Default::default()
        };
        for creator in &mut ret.creators {
            creator.set_subview_sender(ret.subview_sender.clone());
        }
        ret
    }
}

pub static FONT_DB: LazyLock<fontdb::Database> = LazyLock::new(|| {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    db
});

impl eframe::App for MainPage {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let mut request_repaint = true;
        ui.input(|i| {
            if i.viewport().minimized.unwrap_or_default() {
                request_repaint = false;
            }
        });
        if request_repaint {
            ui.request_repaint_after_secs(0.1);
        }
        let cpu_usage = frame.info().cpu_usage.unwrap_or(0.0);
        self.exp_cpu_usage = self.exp_cpu_usage * 31.0 / 32.0 + cpu_usage / 32.0;
        egui::Panel::left("side")
            .min_size(200.0)
            .max_size(280.0)
            .show(ui, |ui| {
                let heading = ui.heading(t!("metatorio.title").to_string());
                if heading.clicked() {
                    self.selected = SelectedSubview::Title;
                }
                ui.label(t!("metatorio.build-hash", GIT_HASH).to_string());
                ui.label(
                    t!(
                        "metatorio.cpu-usage",
                        format!("{:.2}", self.exp_cpu_usage * 1000.0)
                    )
                    .to_string(),
                );
                ui.separator();
                ui.label(
                    t!(
                        "metatorio.current-version",
                        self_update::cargo_crate_version!()
                    )
                    .to_string(),
                );
                if ui
                    .button(t!("metatorio.check-update").to_string())
                    .clicked()
                {
                    self.request_sender
                        .send(NetworkRequest::FetchReleases)
                        .unwrap();
                }
                let response = self.response_receiver.try_recv();
                if let Ok(response) = response {
                    self.suitable_release = response;
                    match self.suitable_release {
                        Ok(_) => {}
                        Err(ref err) => match err {
                            error::AppError::UpToDate => {
                                toast::success(t!("metatorio.up-to-date").to_string());
                            }
                            error::AppError::None => {}
                            err => {
                                toast::error(
                                    t!("metatorio.update-failed", format!("{:?}", err)).to_string(),
                                );
                            }
                        },
                    }
                }
                match &mut self.suitable_release {
                    Ok(release) => {
                        ui.label(t!("metatorio.new-version-available", &release.version));
                        if ui.button(t!("metatorio.update").to_string()).clicked() {
                            self.request_sender
                                .send(NetworkRequest::SelfUpdate)
                                .unwrap();
                        }
                    }
                    Err(err) => match err {
                        error::AppError::None => {}
                        error::AppError::UpToDate => {
                            ui.label(t!("metatorio.up-to-date").to_string());
                        }
                        error::AppError::RestartRequired => {
                            ui.label(t!("metatorio.restart-required").to_string());
                            if ui.button(t!("metatorio.restart-app").to_string()).clicked() {
                                #[allow(clippy::zombie_processes)]
                                std::process::Command::new(std::env::current_exe().unwrap())
                                    .spawn()
                                    .unwrap();
                                ui.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        _err => {
                            ui.label(
                                t!("metatorio.update-failed", format!("{:?}", _err)).to_string(),
                            );
                        }
                    },
                }
                ui.add(egui::Hyperlink::from_label_and_url(
                    t!("metatorio.github-repo").to_string(),
                    "https://github.com/cyx2015s/metatorio-calc",
                ));
                ui.separator();
                self.creators
                    .iter_mut()
                    .enumerate()
                    .for_each(|(i, creator)| {
                        if ui
                            .selectable_label(
                                self.selected == SelectedSubview::Creator(i),
                                creator.name(),
                            )
                            .clicked()
                        {
                            self.selected = SelectedSubview::Creator(i);
                        }
                    });

                while let Ok(subview) = self.subview_receiver.try_recv() {
                    self.planners.push(subview);
                }

                ui.separator();
                let mut i = 0;
                self.planners.retain_mut(|subview| {
                    let label = ui
                        .selectable_label(
                            self.selected == SelectedSubview::Planner(i),
                            subview.name(),
                        )
                        .on_hover_text_at_pointer(subview.description());
                    if label.clicked() {
                        self.selected = SelectedSubview::Planner(i);
                    }
                    i += 1;
                    let mut deleted = false;
                    label.context_menu(|ui| {
                        if ui.button(t!("metatorio.close-planner")).clicked() {
                            deleted = true;
                        }
                    });

                    !deleted
                });
                if let SelectedSubview::Planner(n) = self.selected
                    && n >= self.planners.len()
                {
                    self.selected = SelectedSubview::Title;
                }
                ui.separator();
                if ui.button(t!("metatorio.reload-icons")).clicked() {
                    ui.forget_all_images();
                }
                ui.separator();
                ui.text_edit_singleline(&mut self.font_filter);
                ui.menu_button(t!("metatorio.select-font"), |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(480.0)
                        .max_width(480.0)
                        .show(ui, |ui| {
                            for font in FONT_DB.faces() {
                                let name = &font.post_script_name;
                                if !name
                                    .to_lowercase()
                                    .contains(&self.font_filter.to_lowercase())
                                {
                                    continue;
                                }
                                let is_mono = font.monospaced;
                                let languages = font
                                    .families
                                    .iter()
                                    .map(|(_, lang)| lang.primary_language())
                                    .collect::<Vec<_>>();
                                let label = format!(
                                    "{} {} {:?}",
                                    name,
                                    if is_mono { " [monospace]" } else { "" },
                                    languages,
                                );
                                if ui.selectable_label(false, label).clicked() {
                                    let font_data = FONT_DB.face(font.id).unwrap().source.clone();
                                    let buf = match font_data {
                                        fontdb::Source::Binary(buf) => (*buf).as_ref().to_owned(),
                                        fontdb::Source::File(path) => std::fs::read(path).unwrap(),
                                        fontdb::Source::SharedFile(_, buf) => {
                                            (*buf).as_ref().to_owned()
                                        }
                                    };
                                    update_font(ui, buf);
                                }
                            }
                        })
                });
                ui.separator();
                if ui.button(t!("metatorio.logs")).clicked() {
                    self.selected = SelectedSubview::Logs;
                }
                // if ui.button(t!("metatorio.dump-locale")).clicked() {
                //     eprintln!("{:#?}", get_missing_keys());
                // }
                ui.separator();
                ui.heading("🌐");
                for locale in ["en", "zh-CN"] {
                    if ui.button(locale).clicked() {
                        fust_i18n::set_locale(locale);
                    }
                }
            });
        egui::CentralPanel::default().show(ui, |ui| match self.selected {
            SelectedSubview::Title => {
                ui.label(t!("metatorio.welcome").to_string());
                ui.label(t!("metatorio.welcome-description").to_string());
                ui.label(t!("metatorio.welcome-instructions").to_string());
            }
            SelectedSubview::Creator(n) => self.creators[n].view(ui),
            SelectedSubview::Planner(n) => self.planners[n].view(ui),
            SelectedSubview::Logs => {
                egui_logger::logger_ui()
                    .error_color(egui::Color32::DARK_RED)
                    .warn_color(egui::Color32::ORANGE)
                    .show(ui);
            }
        });
        toast::TOASTS.lock().unwrap().show(ui);
    }
}

fn main() {
    egui_logger::builder()
        .max_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    log::info!("应用程序启动");
    match current_locale::current_locale() {
        Ok(loc) => {
            if loc.contains("zh") {
                fust_i18n::set_locale("zh-CN");
            } else {
                fust_i18n::set_locale("en");
            }
        }
        Err(_) => {
            // 不管了，看中文去吧
            fust_i18n::set_locale("zh-CN");
        }
    }
    update_i18n_ini(
        "zh-CN",
        std::io::Cursor::new(include_str!("../../../locales/zh-CN.cfg")),
    )
    .unwrap();
    update_i18n_ini(
        "en",
        std::io::Cursor::new(include_str!("../../../locales/en.cfg")),
    )
    .unwrap();
    let icon_image = image::load_from_memory(include_bytes!("../../../assets/icon.png")).unwrap();
    eframe::run_native(
        "metatorio",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_maximized(true)
                .with_min_inner_size(egui::Vec2 { x: 800.0, y: 600.0 })
                .with_title(t!("metatorio.title").to_string())
                .with_icon(egui::IconData {
                    rgba: icon_image.to_rgba8().into_raw(),
                    width: icon_image.width(),
                    height: icon_image.height(),
                }),
            #[cfg(feature = "wgpu")]
            renderer: eframe::Renderer::Wgpu,

            ..Default::default()
        },
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);

            let mut query = None;
            let names = ["sarasa", "noto-sans", "noto", "simhei", ""];
            let mut matched = vec![None; names.len()];
            for face in FONT_DB.faces() {
                // log::info!(
                //     "系统字体: {}, 等宽: {}, 语言: {:?}",
                //     face.post_script_name,
                //     face.monospaced,
                //     face.families
                //         .iter()
                //         .map(|(_, lang)| lang.primary_language())
                //         .collect::<Vec<_>>(),
                // );
                if query.is_none()
                    && face.monospaced
                    && face.style == fontdb::Style::Normal
                    && face.weight == fontdb::Weight::NORMAL
                    || face.weight == fontdb::Weight::SEMIBOLD
                {
                    for (_, lang) in &face.families {
                        if lang.primary_language() == "Chinese" {
                            // log::info!("找到系统字体: {}", face.post_script_name);
                            matched[names
                                .iter()
                                .position(|n| face.post_script_name.to_lowercase().contains(n))
                                .unwrap()] = Some(face.id);
                        }
                    }
                }
            }
            query = matched.into_iter().find(|q| q.is_some()).unwrap();
            if let Some(id) = query {
                let font_data = FONT_DB.face(id).unwrap().source.clone();
                let buf = match font_data {
                    fontdb::Source::Binary(buf) => (*buf).as_ref().to_owned(),
                    fontdb::Source::File(path) => std::fs::read(path).unwrap(),
                    fontdb::Source::SharedFile(_, buf) => (*buf).as_ref().to_owned(),
                };
                log::info!("加载系统字体成功");
                update_font(&cc.egui_ctx, buf);
            } else {
                log::warn!("未找到系统等宽字体，使用默认字体");
            }

            cc.egui_ctx.set_theme(egui::Theme::Light);
            cc.egui_ctx.all_styles_mut(|style| {
                style.interaction.tooltip_delay = 0.2;
                style.interaction.tooltip_grace_time = 1.0;
                style.interaction.show_tooltips_only_when_still = false;
                style.spacing.combo_width *= 0.75;
            });

            Ok(Box::new(MainPage::new(cc)))
        }),
    )
    .unwrap();
}

fn update_font(ctx: &egui::Context, buf: Vec<u8>) {
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("main".into(), egui::FontData::from_owned(buf).into());
    fonts.font_data.insert(
        "symbol".into(),
        egui::FontData::from_static(include_bytes!("../../../assets/font.ttf")).into(),
    );
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "main".into());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "symbol".into());
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "main".into());
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "symbol".into());
    ctx.set_fonts(fonts);
}
