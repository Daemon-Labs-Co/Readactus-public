use std::sync::Arc;

use eframe::egui;

use crate::app::{PipelineState, ReadactusApp, Screen};
use readactus_core::{build_plan, connect_source};

pub fn show(app: &mut ReadactusApp, ui: &mut egui::Ui) {
    let Screen::SourceConnection { form, error, connecting } = &mut app.screen else {
        return;
    };

    ui.vertical_centered(|ui| {
        ui.add_space(20.0);
        ui.heading("Source Connection");
        ui.add_space(20.0);
    });

    egui::Grid::new("source_form")
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

    let can_connect = !*connecting
        && !form.host.is_empty()
        && !form.database.is_empty()
        && !form.username.is_empty();
    let is_connecting = *connecting;

    let mut go_back = false;
    let mut do_scan = false;

    ui.horizontal(|ui| {
        if ui.button("Back").clicked() {
            go_back = true;
        }
        if ui.add_enabled(can_connect, egui::Button::new("Scan")).clicked() {
            do_scan = true;
        }
        if is_connecting {
            ui.spinner();
        }
    });

    if go_back {
        app.screen = Screen::Home;
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
