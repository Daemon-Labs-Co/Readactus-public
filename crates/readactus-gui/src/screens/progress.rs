use eframe::egui;

use crate::app::{CopyProgress, ReadactusApp, Screen};
use crate::theme;

pub fn show(app: &mut ReadactusApp, ui: &mut egui::Ui) {
    let (progress, rx) = match &mut app.screen {
        Screen::CopyProgress { progress, rx } => (progress, rx),
        _ => return,
    };

    while let Ok(msg) = rx.try_recv() {
        progress.push(msg);
    }

    ui.add_space(20.0);
    theme::hero(ui, |ui| {
        theme::title(ui, "Copying…");
        theme::caption(ui, "Streaming a safe, tokenised copy to the target");
    });
    ui.add_space(20.0);

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
        ui.colored_label(ui.visuals().error_fg_color, format!("Error: {err}"));
        ui.add_space(12.0);
        if theme::secondary_button(ui, "Back to home", true).clicked() {
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
