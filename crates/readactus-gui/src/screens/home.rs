use eframe::egui;

use crate::app::{ConnTarget, DbForm, ReadactusApp, Screen};
use crate::theme;
use readactus_core::Engine;

pub fn show(app: &mut ReadactusApp, ui: &mut egui::Ui) {
    theme::hero(ui, |ui| {
        theme::brand_header(ui, "Safe, production-quality database copies");
    });
    ui.add_space(28.0);

    theme::card(ui, |ui| {
        theme::caption(ui, "SOURCE ENGINE");
        ui.add_space(6.0);

        let mut engine_idx: usize = match app.source_engine {
            Engine::Postgres => 0,
            Engine::MySql => 1,
        };
        egui::ComboBox::from_id_salt("engine_select")
            .width(ui.available_width())
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

        ui.add_space(16.0);

        // First run (no saved profiles) drops straight into the connection
        // form; once the user has a "My Databases" pool they pick from it.
        let has_profiles = !app.profiles.is_empty();
        let label = if has_profiles {
            "Select source connection"
        } else {
            "Connect to source database"
        };
        if theme::primary_button(ui, label).clicked() {
            app.screen = if has_profiles {
                Screen::Profiles {
                    target: ConnTarget::Source,
                    editor: None,
                    status: None,
                }
            } else {
                Screen::SourceConnection {
                    form: DbForm::for_engine(app.source_engine),
                    error: None,
                    connecting: false,
                }
            };
        }
    });

    ui.add_space(24.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("●").color(theme::accent()).size(10.0));
        ui.add_space(4.0);
        theme::caption(ui, "Registered · Daemon Labs");
    });
}
