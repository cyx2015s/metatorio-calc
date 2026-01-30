use egui_toast::ToastKind;

lazy_static::lazy_static! {
    pub static ref TOASTS: std::sync::Mutex<egui_toast::Toasts> = std::sync::Mutex::new(setup_toasts());
}

pub fn setup_toasts() -> egui_toast::Toasts {
    egui_toast::Toasts::new()
        .anchor(egui::Align2::RIGHT_TOP, (-10.0, 10.0))
        .custom_contents(ToastKind::Custom(0), |ui, toast| {
            let _ = toast;
            ui.response().clone()
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
