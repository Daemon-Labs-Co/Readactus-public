use eframe::egui;
use readactus_license::{activate, ISSUER_PUBLIC_KEY_B32};

use crate::app::{ReadactusApp, Screen};

pub fn show(app: &mut ReadactusApp, ui: &mut egui::Ui) {
    let Screen::Activate { key_input, error } = &mut app.screen else {
        return;
    };

    ui.add_space(80.0);
    ui.vertical_centered(|ui| {
        ui.heading("Readactus");
        ui.add_space(8.0);
        ui.label("A Daemon Labs product");
    });
    ui.add_space(40.0);

    ui.vertical_centered(|ui| {
        ui.label("Enter your registration key to continue:");
    });
    ui.add_space(8.0);

    ui.vertical_centered(|ui| {
        let input = egui::TextEdit::singleline(key_input)
            .hint_text("RDX1-...")
            .desired_width(400.0);
        ui.add(input);
    });
    ui.add_space(12.0);

    if let Some(err) = error {
        ui.vertical_centered(|ui| {
            ui.colored_label(egui::Color32::from_rgb(220, 50, 50), err.as_str());
        });
        ui.add_space(8.0);
    }

    let mut do_activate = false;
    ui.vertical_centered(|ui| {
        if ui.button("Activate").clicked() && !key_input.is_empty() {
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
