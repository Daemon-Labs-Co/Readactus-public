use eframe::egui;

use crate::app::{ReadactusApp, Screen};

pub fn show(app: &mut ReadactusApp, ui: &mut egui::Ui) {
    let report = match &app.screen {
        Screen::Results { report } => report,
        _ => return,
    };

    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        ui.heading("Copy Complete");
        ui.add_space(20.0);

        ui.label(format!(
            "{} table(s), {} total row(s)",
            report.tables.len(),
            report.total_rows,
        ));
        ui.add_space(12.0);
    });

    egui::Grid::new("results_table")
        .num_columns(3)
        .spacing([16.0, 6.0])
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Table");
            ui.strong("Rows");
            ui.strong("Columns Transformed");
            ui.end_row();

            for table in &report.tables {
                ui.label(format!("{}.{}", table.table.schema, table.table.name));
                ui.label(format!("{}", table.rows_copied));
                ui.label(format!("{}", table.columns_transformed));
                ui.end_row();
            }
        });

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(12.0);

    if ui.button("Start another copy").clicked() {
        app.pipeline = None;
        app.screen = Screen::Home;
    }
}
