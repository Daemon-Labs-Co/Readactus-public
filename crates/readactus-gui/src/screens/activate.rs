use eframe::egui;
use readactus_license::{activate, ISSUER_PUBLIC_KEY_B32};

use crate::app::{ReadactusApp, Screen};
use crate::theme;

pub fn show(app: &mut ReadactusApp, ui: &mut egui::Ui) {
    let Screen::Activate { key_input, error } = &mut app.screen else {
        return;
    };

    ui.add_space(24.0);
    theme::hero(ui, |ui| {
        theme::brand_header(ui, "A Daemon Labs product");
    });
    ui.add_space(28.0);

    let mut do_activate = false;
    theme::card(ui, |ui| {
        theme::caption(ui, "REGISTRATION KEY");
        ui.add_space(6.0);
        ui.label("Enter your registration key to continue:");
        ui.add_space(10.0);

        let input = egui::TextEdit::multiline(key_input)
            .hint_text("RDX1-…")
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY)
            .desired_rows(4);
        ui.add(input);

        if let Some(err) = error {
            ui.add_space(10.0);
            ui.colored_label(ui.visuals().error_fg_color, err.as_str());
        }

        ui.add_space(16.0);
        if theme::primary_button(ui, "Activate").clicked() && !key_input.is_empty() {
            do_activate = true;
        }
    });

    if do_activate {
        let key = match &app.screen {
            Screen::Activate { key_input, .. } => key_input.trim().to_string(),
            _ => return,
        };
        match activate(&key, ISSUER_PUBLIC_KEY_B32, None) {
            Ok(activation) => {
                tracing::info!("activated key {} (tier: {})", activation.key_id, activation.tier);
                app.reload_entitlements();
                app.screen = Screen::Home;
            }
            Err(e) => {
                if let Screen::Activate { error, .. } = &mut app.screen {
                    *error = Some(format!("{e}"));
                }
            }
        }
    }
}
