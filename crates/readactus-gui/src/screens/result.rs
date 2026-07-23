use eframe::egui;

use crate::app::{ReadactusApp, Screen};
use crate::theme;

pub fn show(app: &mut ReadactusApp, ui: &mut egui::Ui) {
    let report = match &app.screen {
        Screen::Results { report } => report,
        _ => return,
    };

    ui.add_space(12.0);
    theme::hero(ui, |ui| {
        ui.label(egui::RichText::new("✓").color(theme::accent()).size(34.0));
        theme::title(ui, "Copy Complete");
        theme::caption(
            ui,
            &format!(
                "{} table(s) · {} total row(s) copied",
                report.tables.len(),
                report.total_rows,
            ),
        );
    });
    ui.add_space(20.0);

    theme::card(ui, |ui| {
        egui::Grid::new("results_table")
            .num_columns(3)
            .spacing([16.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                ui.strong("Table");
                ui.strong("Rows");
                ui.strong("Transformed");
                ui.end_row();

                for table in &report.tables {
                    ui.label(format!("{}.{}", table.table.schema, table.table.name));
                    ui.label(format!("{}", table.rows_copied));
                    ui.label(format!("{}", table.columns_transformed));
                    ui.end_row();
                }
            });
    });

    ui.add_space(20.0);

    if theme::primary_button(ui, "Start another copy").clicked() {
        app.pipeline = None;
        app.screen = Screen::Home;
    }
}
