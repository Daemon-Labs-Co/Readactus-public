use std::sync::{mpsc, Arc};

use eframe::egui;

use crate::app::{ConnTarget, CopyProgress, ProfileEditor, ReadactusApp, Screen};
use crate::screens::db_form_fields;
use crate::theme;
use readactus_core::{connect_source, connect_target, run_copy};
use readactus_transform::{RunKey, Tokenizer};

pub fn show(app: &mut ReadactusApp, ui: &mut egui::Ui) {
    let Screen::TargetConnection { form, error, connecting } = &mut app.screen else {
        return;
    };

    theme::hero(ui, |ui| {
        theme::title(ui, "Target Connection");
        theme::caption(ui, "The destination database for the safe copy");
    });
    ui.add_space(20.0);

    theme::card(ui, |ui| {
        db_form_fields(ui, "target_form", form);
    });

    ui.add_space(12.0);

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
    let mut save_as_profile = false;

    ui.horizontal(|ui| {
        if theme::secondary_button(ui, "Back", true).clicked() {
            go_back = true;
        }
        if theme::secondary_button(ui, "Save as profile", !is_connecting).clicked() {
            save_as_profile = true;
        }
        if theme::primary_button_enabled(ui, "Start copy", can_start).clicked() {
            do_copy = true;
        }
        if is_connecting {
            ui.add_space(4.0);
            ui.spinner();
        }
    });

    if go_back {
        app.screen = Screen::PlanReview { threshold: 0.7 };
        return;
    }

    if save_as_profile {
        if let Screen::TargetConnection { form, .. } = &app.screen {
            let form = form.clone();
            app.screen = Screen::Profiles {
                target: ConnTarget::Target,
                editor: Some(ProfileEditor {
                    name: String::new(),
                    form,
                    editing_id: None,
                    error: None,
                    testing: false,
                }),
                status: None,
            };
        }
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
