lazy_static::lazy_static! {
    pub static ref TOASTS: std::sync::Mutex<egui_toast::Toasts> = std::sync::Mutex::new(egui_toast::Toasts::new().anchor(egui::Align2::RIGHT_TOP, (-10.0, 10.0)));
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
        options: egui_toast::ToastOptions::default().duration(None),
        style: egui_toast::ToastStyle::default(),
    });
}
