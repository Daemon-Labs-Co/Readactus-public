use eframe::egui;

use crate::app::DbForm;

pub mod activate;
pub mod home;
pub mod profiles;
pub mod source;
pub mod review;
pub mod target;
pub mod progress;
pub mod result;

/// The shared connection-details form used by the source and target screens.
pub fn db_form_fields(ui: &mut egui::Ui, id: &str, form: &mut DbForm) {
    let field_width = ui.available_width() - 120.0;
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([16.0, 10.0])
        .min_col_width(96.0)
        .show(ui, |ui| {
            ui.label("Host");
            ui.add(egui::TextEdit::singleline(&mut form.host).desired_width(field_width));
            ui.end_row();

            ui.label("Port");
            ui.add(egui::TextEdit::singleline(&mut form.port).desired_width(90.0));
            ui.end_row();

            ui.label("Database");
            ui.add(egui::TextEdit::singleline(&mut form.database).desired_width(field_width));
            ui.end_row();

            ui.label("Username");
            ui.add(egui::TextEdit::singleline(&mut form.username).desired_width(field_width));
            ui.end_row();

            ui.label("Password");
            ui.add(
                egui::TextEdit::singleline(&mut form.password)
                    .password(true)
                    .desired_width(field_width),
            );
            ui.end_row();

            ui.label("TLS");
            ui.checkbox(&mut form.use_tls, "Encrypt connection");
            ui.end_row();
        });
}
