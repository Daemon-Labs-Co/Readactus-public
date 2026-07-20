use eframe::egui;

use crate::app::{DbForm, ReadactusApp, Screen};
use readactus_core::ColumnAction;
use readactus_detect::{kind_label, PiiKind};

pub fn show(app: &mut ReadactusApp, ui: &mut egui::Ui) {
    let threshold = match &mut app.screen {
        Screen::PlanReview { threshold } => threshold,
        _ => return,
    };

    let pipeline = match &mut app.pipeline {
        Some(p) => p,
        None => {
            ui.label("No scan data available.");
            if ui.button("Back to home").clicked() {
                app.screen = Screen::Home;
            }
            return;
        }
    };

    ui.vertical_centered(|ui| {
        ui.add_space(12.0);
        ui.heading("Review Transformation Plan");
        ui.add_space(4.0);
        let total_cols: usize = pipeline.plan.tables.iter().map(|t| t.columns.len()).sum();
        let transformed: usize = pipeline
            .plan
            .tables
            .iter()
            .flat_map(|t| &t.columns)
            .filter(|c| matches!(c.action, ColumnAction::Tokenize(_)))
            .count();
        ui.label(format!(
            "{} table(s), {} column(s) total, {} to be transformed",
            pipeline.plan.tables.len(),
            total_cols,
            transformed,
        ));
    });

    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.label("Confidence threshold:");
        ui.add(egui::Slider::new(threshold, 0.1..=1.0).step_by(0.05));
    });

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        for table_plan in &mut pipeline.plan.tables {
            let table_label = format!("{}.{}", table_plan.table.schema, table_plan.table.name);
            egui::CollapsingHeader::new(egui::RichText::new(&table_label).strong())
                .default_open(true)
                .show(ui, |ui| {
                    egui::Grid::new(format!("plan_{}", table_label))
                        .num_columns(4)
                        .spacing([16.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("Column");
                            ui.strong("PII Kind");
                            ui.strong("Confidence");
                            ui.strong("Transform");
                            ui.end_row();

                            for col_plan in &mut table_plan.columns {
                                ui.label(&col_plan.column);

                                let kind_text = match &col_plan.action {
                                    ColumnAction::Tokenize(kind) => kind_label(kind),
                                    ColumnAction::Passthrough => match &col_plan.finding {
                                        Some(f) => kind_label(&f.kind),
                                        None => "-",
                                    },
                                };
                                ui.label(kind_text);

                                let confidence = col_plan
                                    .finding
                                    .as_ref()
                                    .map(|f| format!("{:.0}%", f.confidence * 100.0))
                                    .unwrap_or_else(|| "-".into());
                                ui.label(&confidence);

                                let mut enabled = matches!(col_plan.action, ColumnAction::Tokenize(_));
                                if ui.checkbox(&mut enabled, "").changed() {
                                    col_plan.action = if enabled {
                                        let kind = col_plan
                                            .finding
                                            .as_ref()
                                            .map(|f| f.kind.clone())
                                            .unwrap_or(PiiKind::Credential);
                                        ColumnAction::Tokenize(kind)
                                    } else {
                                        ColumnAction::Passthrough
                                    };
                                }

                                ui.end_row();
                            }
                        });
                });
        }
    });

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        if ui.button("Back").clicked() {
            app.screen = Screen::SourceConnection {
                form: app.source_form.clone(),
                error: None,
                connecting: false,
            };
        }

        if ui.button("Continue to target").clicked() {
            let default_port = match app.source_engine {
                readactus_core::Engine::Postgres => "5432",
                readactus_core::Engine::MySql => "3306",
            };
            app.screen = Screen::TargetConnection {
                form: DbForm {
                    engine: app.source_engine,
                    port: default_port.into(),
                    ..DbForm::default()
                },
                error: None,
                connecting: false,
            };
        }
    });
}
