mod app;
mod screens;

use eframe::NativeOptions;
use eframe::egui;

fn main() -> eframe::Result {
    tracing_subscriber::fmt::init();

    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 640.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Readactus",
        options,
        Box::new(|cc| Ok(Box::new(app::ReadactusApp::new(cc)))),
    )
}
