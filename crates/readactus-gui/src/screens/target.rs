use std::sync::{mpsc, Arc};

use eframe::egui;

use crate::app::{CopyProgress, ReadactusApp, Screen};
use readactus_core::{connect_source, connect_target, run_copy};
use readactus_transform::{RunKey, Tokenizer};

pub fn show(app: &mut ReadactusApp, ui: &mut egui::Ui) {
    let Screen::TargetConnection { form, error, connecting } = &mut app.screen else {
        return;
    };

    ui.vertical_centered(|ui| {
        ui.add_space(20.0);
        ui.heading("Target Connection");
        ui.add_space(4.0);
        ui.label("The destination database for the safe copy");
        ui.add_space(20.0);
    });

    egui::Grid::new("target_form")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label("Host:");
            ui.add(egui::TextEdit::singleline(&mut form.host).desired_width(300.0));
            ui.end_row();

            ui.label("Port:");
            ui.add(egui::TextEdit::singleline(&mut form.port).desired_width(80.0));
            ui.end_row();

            ui.label("Database:");
            ui.add(egui::TextEdit::singleline(&mut form.database).desired_width(300.0));
            ui.end_row();

            ui.label("Username:");
            ui.add(egui::TextEdit::singleline(&mut form.username).desired_width(300.0));
            ui.end_row();

            ui.label("Password:");
            ui.add(
                egui::TextEdit::singleline(&mut form.password)
                    .password(true)
                    .desired_width(300.0),
            );
            ui.end_row();

            ui.label("TLS:");
            ui.checkbox(&mut form.use_tls, "Encrypt connection");
            ui.end_row();
        });

    ui.add_space(16.0);

    if let Some(err) = error {
        ui.colored_label(ui.visuals().error_fg_color, err.as_str());
        ui.add_space(8.0);
    }

    let can_start = !*connecting
        && !form.host.is_empty()
        && !form.database.is_empty()
        && !form.username.is_empty();
    let is_connecting = *connecting;

    let mut go_back = false;
    let mut do_copy = false;

    ui.horizontal(|ui| {
        if ui.button("Back").clicked() {
            go_back = true;
        }
        if ui.add_enabled(can_start, egui::Button::new("Start copy")).clicked() {
            do_copy = true;
        }
        if is_connecting {
            ui.spinner();
        }
    });

    if go_back {
        app.screen = Screen::PlanReview { threshold: 0.7 };
        return;
    }

    if do_copy {
        let Screen::TargetConnection { form, error, .. } = &mut app.screen else {
            return;
        };
        let target_config = match form.to_config() {
            Ok(c) => c,
            Err(e) => {
                *error = Some(e.to_string());
                return;
            }
        };
        let target_engine = form.engine;
        let source_config = match app.source_form.to_config() {
            Ok(c) => c,
            Err(e) => {
                if let Screen::TargetConnection { error, .. } = &mut app.screen {
                    *error = Some(e.to_string());
                }
                return;
            }
        };
        let source_engine = app.source_form.engine;
        let entitlements = app.entitlements.clone();
        let rt = Arc::clone(&app.tokio_rt);

        let pipeline = match &app.pipeline {
            Some(p) => p,
            None => {
                if let Screen::TargetConnection { error, .. } = &mut app.screen {
                    *error = Some("no scan data — go back and re-scan".into());
                }
                return;
            }
        };
        let catalog = pipeline.catalog.clone();
        let plan = pipeline.plan.clone();

        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            rt.block_on(async {
                let source = match connect_source(source_engine, source_config).await {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(CopyProgress::Failed(format!("{e}")));
                        return;
                    }
                };
                let target = match connect_target(target_engine, target_config).await {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(CopyProgress::Failed(format!("{e}")));
                        return;
                    }
                };

                let tokenizer = Arc::new(Tokenizer::new(RunKey::generate()));
                match run_copy(&*source, &*target, &catalog, &plan, tokenizer, &entitlements).await {
                    Ok(report) => {
                        let _ = tx.send(CopyProgress::Done(report));
                    }
                    Err(e) => {
                        let _ = tx.send(CopyProgress::Failed(format!("{e}")));
                    }
                }
            });
        });

        app.screen = Screen::CopyProgress {
            progress: Vec::new(),
            rx,
        };
    }
}
