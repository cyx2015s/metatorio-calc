use eframe::egui;

fn main() -> eframe::Result {
    eframe::run_native(
        "Metatorio",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_min_inner_size([960.0, 640.0])
                .with_maximized(true),
            ..Default::default()
        },
        Box::new(|_cc| Ok(Box::new(metatorio_ui::view::MetatorioApp::default()))),
    )
}
