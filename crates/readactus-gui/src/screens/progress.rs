use eframe::egui;

use crate::app::{CopyProgress, ReadactusApp, Screen};

pub fn show(app: &mut ReadactusApp, ui: &mut egui::Ui) {
    let (progress, rx) = match &mut app.screen {
        Screen::CopyProgress { progress, rx } => (progress, rx),
        _ => return,
    };

    while let Ok(msg) = rx.try_recv() {
        progress.push(msg);
    }

    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        ui.heading("Copying...");
        ui.add_space(20.0);
    });

    let mut finished = false;
    let mut failed: Option<String> = None;

    for entry in progress.iter() {
        match entry {
            CopyProgress::Table { schema, table, rows } => {
                ui.label(format!("  {schema}.{table}: {rows} row(s)"));
            }
            CopyProgress::Done(_) => {
                finished = true;
            }
            CopyProgress::Failed(msg) => {
                failed = Some(msg.clone());
            }
        }
    }

    if let Some(err) = &failed {
        ui.add_space(16.0);
        ui.colored_label(egui::Color32::from_rgb(220, 50, 50), format!("Error: {err}"));
        ui.add_space(12.0);
        if ui.button("Back to home").clicked() {
            app.pipeline = None;
            app.screen = Screen::Home;
        }
        return;
    }

    if finished {
        let report = progress
            .iter()
            .find_map(|p| match p {
                CopyProgress::Done(r) => Some(r.clone()),
                _ => None,
            })
            .unwrap();
        app.screen = Screen::Results { report };
        return;
    }

    ui.add_space(20.0);
    ui.spinner();

    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(100));
}
