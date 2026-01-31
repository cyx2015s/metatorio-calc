use egui_toast::ToastKind;

use crate::update::{DownloadProgress, get_download_progress};

lazy_static::lazy_static! {
    pub static ref TOASTS: std::sync::Mutex<egui_toast::Toasts> = std::sync::Mutex::new(setup_toasts());
}

pub fn setup_toasts() -> egui_toast::Toasts {
    egui_toast::Toasts::new()
        .anchor(egui::Align2::RIGHT_TOP, (-10.0, 10.0))
        .custom_contents(ToastKind::Custom(0), |ui, toast| {
            match get_download_progress() {
                DownloadProgress::Pending => {
                    log::info!("更新下载等待中");
                    toast.close();
                    return ui.response().clone();
                }
                DownloadProgress::InProgress(current, total) => {
                    let inner_margin = 10.0;
                    let frame = egui::Frame::window(ui.style());
                    let response = frame
                        .inner_margin(inner_margin)
                        .stroke(egui::Stroke::NONE)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let progress = if total == 0 {
                                    0.0
                                } else {
                                    current as f32 / total as f32
                                };
                                ui.vertical_centered(|ui| {
                                    ui.label("正在下载更新...");
                                    ui.add(
                                        egui::ProgressBar::new(progress)
                                            .fill(if current == total && total != 0 {
                                                egui::Color32::GREEN
                                            } else {
                                                egui::Color32::LIGHT_BLUE
                                            })
                                            .desired_width(128.0)
                                            .text(format!(
                                                "{} KB/ {} KB",
                                                (current) / 1024,
                                                (total) / 1024,
                                            )),
                                    );
                                });
                            })
                        })
                        .response;

                    // Draw the frame's stroke last
                    let frame_shape = egui::Shape::Rect(egui::epaint::RectShape::stroke(
                        response.rect,
                        frame.corner_radius,
                        ui.visuals().window_stroke,
                        egui::StrokeKind::Inside,
                    ));
                    ui.painter().add(frame_shape);

                    return response;
                }
                DownloadProgress::Completed => {
                    log::info!("更新下载完成");
                    toast.kind = ToastKind::Success;
                    toast.text = "更新下载完成。".into();
                    toast.options.duration_in_seconds(3.0);
                    return ui.response().clone();
                }
            }
        })
}

pub fn success(text: impl Into<egui::WidgetText>) {
    TOASTS.lock().unwrap().add(egui_toast::Toast {
        kind: egui_toast::ToastKind::Success,
        text: text.into(),
        options: egui_toast::ToastOptions::default().duration_in_seconds(3.0),
        style: egui_toast::ToastStyle {
            success_icon: "√".into(),
            ..Default::default()
        },
    });
}

pub fn info(text: impl Into<egui::WidgetText>) {
    TOASTS.lock().unwrap().add(egui_toast::Toast {
        kind: egui_toast::ToastKind::Info,
        text: text.into(),
        options: egui_toast::ToastOptions::default().duration_in_seconds(3.0),
        style: egui_toast::ToastStyle::default(),
    });
}

pub fn error(text: impl Into<egui::WidgetText>) {
    TOASTS.lock().unwrap().add(egui_toast::Toast {
        kind: egui_toast::ToastKind::Error,
        text: text.into(),
        options: egui_toast::ToastOptions::default().duration_in_seconds(10.0),
        style: egui_toast::ToastStyle::default(),
    });
}

pub fn download() {
    log::info!("显示下载更新 toast");
    TOASTS.lock().unwrap().add(egui_toast::Toast {
        kind: egui_toast::ToastKind::Custom(0),
        text: "".into(),
        options: egui_toast::ToastOptions::default()
            .show_progress(false)
            .show_icon(false)
            .duration(None),
        style: egui_toast::ToastStyle::default(),
    });
}
