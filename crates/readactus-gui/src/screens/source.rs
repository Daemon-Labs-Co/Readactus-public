use std::sync::Arc;

use eframe::egui;

use crate::app::{ConnTarget, PipelineState, ProfileEditor, ReadactusApp, Screen};
use crate::screens::db_form_fields;
use crate::theme;
use readactus_core::{build_plan, connect_source};

pub fn show(app: &mut ReadactusApp, ui: &mut egui::Ui) {
    let Screen::SourceConnection { form, error, connecting } = &mut app.screen else {
        return;
    };

    theme::hero(ui, |ui| {
        theme::title(ui, "Source Connection");
        theme::caption(ui, "The database to read and scan for PII");
    });
    ui.add_space(20.0);

    theme::card(ui, |ui| {
        db_form_fields(ui, "source_form", form);
    });

    ui.add_space(12.0);

    if let Some(err) = error {
        ui.colored_label(ui.visuals().error_fg_color, err.as_str());
        ui.add_space(8.0);
    }

    let can_connect = !*connecting
        && !form.host.is_empty()
        && !form.database.is_empty()
        && !form.username.is_empty();
    let is_connecting = *connecting;

    let mut go_back = false;
    let mut do_scan = false;
    let mut save_as_profile = false;

    ui.horizontal(|ui| {
        if theme::secondary_button(ui, "Back", true).clicked() {
            go_back = true;
        }
        if theme::secondary_button(ui, "Save as profile", !is_connecting).clicked() {
            save_as_profile = true;
        }
        if theme::primary_button_enabled(ui, "Scan", can_connect).clicked() {
            do_scan = true;
        }
        if is_connecting {
            ui.add_space(4.0);
            ui.spinner();
        }
    });

    if go_back {
        app.screen = Screen::Home;
        return;
    }

    if save_as_profile {
        // Hand the current details to the "My Databases" editor to name and
        // save; returning to it lands on the list where they can pick it.
        if let Screen::SourceConnection { form, .. } = &app.screen {
            let form = form.clone();
            app.screen = Screen::Profiles {
                target: ConnTarget::Source,
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

    if do_scan {
        let Screen::SourceConnection { form, error, connecting } = &mut app.screen else {
            return;
        };
        let config = match form.to_config() {
            Ok(c) => c,
            Err(e) => {
                *error = Some(e.to_string());
                return;
            }
        };
        let engine = form.engine;
        let rt = Arc::clone(&app.tokio_rt);
        *connecting = true;

        let result = rt.block_on(async {
            let conn = connect_source(engine, config).await?;
            let catalog = conn.reflect_schema().await?;
            let (plan, findings) = build_plan(&catalog, 0.5);
            Ok::<_, readactus_core::ReadactusError>((catalog, plan, findings))
        });

        let Screen::SourceConnection { form, error, connecting } = &mut app.screen else {
            return;
        };
        match result {
            Ok((catalog, plan, findings)) => {
                app.source_form = form.clone();
                app.pipeline = Some(PipelineState { catalog, plan, findings });
                app.screen = Screen::PlanReview { threshold: 0.7 };
            }
            Err(e) => {
                *connecting = false;
                *error = Some(format!("{e}"));
            }
        }
    }
}
