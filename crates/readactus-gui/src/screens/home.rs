use eframe::egui;

use crate::app::{DbForm, ReadactusApp, Screen};
use readactus_core::Engine;

pub fn show(app: &mut ReadactusApp, ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        ui.heading("Readactus");
        ui.add_space(4.0);
        ui.label("Safe, production-quality database copies");
        ui.add_space(40.0);

        ui.label("Source engine:");
        ui.add_space(4.0);

        let mut engine_idx: usize = match app.source_engine {
            Engine::Postgres => 0,
            Engine::MySql => 1,
        };
        egui::ComboBox::from_id_salt("engine_select")
            .width(200.0)
            .show_index(ui, &mut engine_idx, 2, |i| {
                match i {
                    0 => "PostgreSQL",
                    1 => "MySQL / MariaDB",
                    _ => unreachable!(),
                }
                .to_string()
            });
        app.source_engine = match engine_idx {
            0 => Engine::Postgres,
            _ => Engine::MySql,
        };

        ui.add_space(24.0);

        if ui.button("Connect to source database").clicked() {
            let default_port = match app.source_engine {
                Engine::Postgres => "5432",
                Engine::MySql => "3306",
            };
            app.screen = Screen::SourceConnection {
                form: DbForm {
                    engine: app.source_engine,
                    port: default_port.into(),
                    ..DbForm::default()
                },
                error: None,
                connecting: false,
            };
        }

        ui.add_space(40.0);
        ui.separator();
        ui.add_space(8.0);
        ui.colored_label(ui.visuals().warn_fg_color, "Registered");
    });
}
