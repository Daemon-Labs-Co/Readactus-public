//! The "My Databases" screen — manage the shared pool of connection profiles
//! and pick one to feed the source or target step.
//!
//! It renders one of two views: a **list** of saved profiles (with Use / Edit /
//! Test / Delete per row and a "+ New connection" action), or an **editor**
//! (name + connection fields + Test + Save) when adding or editing. Which view
//! is shown is decided by whether `Screen::Profiles.editor` is `Some`.

use std::sync::Arc;

use eframe::egui;
use egui::RichText;

use crate::app::{ConnTarget, DbForm, ProfileEditor, ReadactusApp, Screen};
use crate::profiles::{self, ConnectionProfile};
use crate::screens::db_form_fields;
use crate::theme;
use readactus_core::{connect_source, Engine};

pub fn show(app: &mut ReadactusApp, ui: &mut egui::Ui) {
    let editing = matches!(&app.screen, Screen::Profiles { editor: Some(_), .. });
    if editing {
        show_editor(app, ui);
    } else {
        show_list(app, ui);
    }
}

// ---------------------------------------------------------------------------
// List view
// ---------------------------------------------------------------------------

enum ListAction {
    New,
    Use(usize),
    Edit(usize),
    Test(usize),
    Delete(usize),
    Back,
}

fn show_list(app: &mut ReadactusApp, ui: &mut egui::Ui) {
    let target = match &app.screen {
        Screen::Profiles { target, .. } => *target,
        _ => return,
    };

    theme::hero(ui, |ui| {
        theme::title(ui, "My Databases");
        theme::caption(
            ui,
            match target {
                ConnTarget::Source => "Choose a saved connection to scan, or add a new one",
                ConnTarget::Target => "Choose a saved connection for the safe copy, or add a new one",
            },
        );
    });
    ui.add_space(20.0);

    let mut action: Option<ListAction> = None;

    theme::card(ui, |ui| {
        if app.profiles.is_empty() {
            theme::caption(ui, "No saved connections yet. Add one to get started.");
        } else {
            let count = app.profiles.len();
            for (i, p) in app.profiles.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&p.name).strong().size(16.0));
                        theme::caption(
                            ui,
                            &format!(
                                "{} · {}:{}/{}",
                                engine_label(p.engine),
                                p.host,
                                p.port,
                                p.database
                            ),
                        );
                    });
                    // Right-aligned row actions.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::secondary_button(ui, "Delete", true).clicked() {
                            action = Some(ListAction::Delete(i));
                        }
                        if theme::secondary_button(ui, "Test", true).clicked() {
                            action = Some(ListAction::Test(i));
                        }
                        if theme::secondary_button(ui, "Edit", true).clicked() {
                            action = Some(ListAction::Edit(i));
                        }
                        if theme::primary_button(ui, "Use").clicked() {
                            action = Some(ListAction::Use(i));
                        }
                    });
                });
                if i + 1 < count {
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);
                }
            }
        }
    });

    ui.add_space(12.0);

    if let Screen::Profiles { status: Some(msg), .. } = &app.screen {
        ui.label(RichText::new(msg).color(theme::muted(ui)));
        ui.add_space(8.0);
    }

    ui.horizontal(|ui| {
        if theme::secondary_button(ui, "Back", true).clicked() {
            action = Some(ListAction::Back);
        }
        if theme::primary_button(ui, "+ New connection").clicked() {
            action = Some(ListAction::New);
        }
    });

    match action {
        Some(ListAction::Back) => {
            app.screen = match target {
                ConnTarget::Source => Screen::Home,
                ConnTarget::Target => Screen::PlanReview { threshold: 0.7 },
            };
        }
        Some(ListAction::New) => {
            app.screen = Screen::Profiles {
                target,
                editor: Some(ProfileEditor {
                    name: String::new(),
                    form: DbForm::for_engine(Engine::Postgres),
                    editing_id: None,
                    error: None,
                    testing: false,
                }),
                status: None,
            };
        }
        Some(ListAction::Edit(i)) => {
            let p = &app.profiles[i];
            let editor = ProfileEditor {
                name: p.name.clone(),
                form: p.to_form(),
                editing_id: Some(p.id.clone()),
                error: None,
                testing: false,
            };
            app.screen = Screen::Profiles {
                target,
                editor: Some(editor),
                status: None,
            };
        }
        Some(ListAction::Use(i)) => {
            let form = app.profiles[i].to_form();
            app.screen = match target {
                ConnTarget::Source => Screen::SourceConnection {
                    form,
                    error: None,
                    connecting: false,
                },
                ConnTarget::Target => Screen::TargetConnection {
                    form,
                    error: None,
                    connecting: false,
                },
            };
        }
        Some(ListAction::Delete(i)) => {
            let removed = app.profiles.remove(i);
            profiles::delete_password(&removed.id);
            app.save_profiles();
            app.screen = Screen::Profiles {
                target,
                editor: None,
                status: Some(format!("Deleted “{}”.", removed.name)),
            };
        }
        Some(ListAction::Test(i)) => {
            let status = test_connection(app, &app.profiles[i].to_form());
            if let Screen::Profiles { status: s, .. } = &mut app.screen {
                *s = Some(status);
            }
        }
        None => {}
    }
}

// ---------------------------------------------------------------------------
// Editor view
// ---------------------------------------------------------------------------

fn show_editor(app: &mut ReadactusApp, ui: &mut egui::Ui) {
    let (target, is_edit) = match &app.screen {
        Screen::Profiles { target, editor, .. } => {
            (*target, editor.as_ref().is_some_and(|e| e.editing_id.is_some()))
        }
        _ => return,
    };

    theme::hero(ui, |ui| {
        theme::title(ui, if is_edit { "Edit connection" } else { "New connection" });
        theme::caption(ui, "Passwords are stored in your operating system's secret store");
    });
    ui.add_space(20.0);

    let mut do_cancel = false;
    let mut do_test = false;
    let mut do_save = false;

    if let Screen::Profiles { editor: Some(ed), .. } = &mut app.screen {
        theme::card(ui, |ui| {
            egui::Grid::new("profile_name")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .min_col_width(96.0)
                .show(ui, |ui| {
                    ui.label("Name");
                    ui.add(
                        egui::TextEdit::singleline(&mut ed.name)
                            .hint_text("e.g. Staging Postgres")
                            .desired_width(ui.available_width() - 120.0),
                    );
                    ui.end_row();

                    ui.label("Engine");
                    engine_combo(ui, &mut ed.form.engine);
                    ui.end_row();
                });
            ui.add_space(6.0);
            db_form_fields(ui, "profile_editor", &mut ed.form);
        });

        ui.add_space(12.0);
        if let Some(err) = &ed.error {
            ui.colored_label(ui.visuals().error_fg_color, err.as_str());
            ui.add_space(8.0);
        }

        let can_save = !ed.name.trim().is_empty()
            && !ed.form.host.is_empty()
            && !ed.form.database.is_empty()
            && !ed.form.username.is_empty();
        let testing = ed.testing;

        ui.horizontal(|ui| {
            if theme::secondary_button(ui, "Cancel", true).clicked() {
                do_cancel = true;
            }
            if theme::secondary_button(ui, "Test connection", !testing).clicked() {
                do_test = true;
            }
            if theme::primary_button_enabled(ui, "Save", can_save).clicked() {
                do_save = true;
            }
            if testing {
                ui.add_space(4.0);
                ui.spinner();
            }
        });
    }

    if do_cancel {
        app.screen = Screen::Profiles {
            target,
            editor: None,
            status: None,
        };
        return;
    }

    if do_test {
        // Read the form out, run the (blocking) connect, then write the result
        // back into the editor's error slot.
        let form = match &app.screen {
            Screen::Profiles { editor: Some(ed), .. } => ed.form.clone(),
            _ => return,
        };
        if let Screen::Profiles { editor: Some(ed), .. } = &mut app.screen {
            ed.testing = true;
        }
        let outcome = test_connection(app, &form);
        if let Screen::Profiles { editor: Some(ed), .. } = &mut app.screen {
            ed.testing = false;
            ed.error = Some(outcome);
        }
        return;
    }

    if do_save {
        save_editor(app, target);
    }
}

/// Persist the editor's profile: mint a new one or update the existing id in
/// place, write the password to the OS secret store, save the JSON, and return
/// to the list.
fn save_editor(app: &mut ReadactusApp, target: ConnTarget) {
    let (name, form, editing_id) = match &app.screen {
        Screen::Profiles { editor: Some(ed), .. } => {
            (ed.name.trim().to_string(), ed.form.clone(), ed.editing_id.clone())
        }
        _ => return,
    };

    let id = match editing_id {
        Some(id) => {
            if let Some(p) = app.profiles.iter_mut().find(|p| p.id == id) {
                p.update_from_form(name.clone(), &form);
            }
            id
        }
        None => {
            let profile = ConnectionProfile::from_form(name.clone(), &form);
            let id = profile.id.clone();
            app.profiles.push(profile);
            id
        }
    };

    // Password lives in the secret store, never in connections.json.
    let secret_note = if form.password.is_empty() {
        profiles::delete_password(&id);
        String::new()
    } else {
        match profiles::set_password(&id, &form.password) {
            Ok(()) => String::new(),
            Err(e) => {
                tracing::warn!("could not store password for “{name}”: {e}");
                " (password not remembered — secret store unavailable)".to_string()
            }
        }
    };

    app.save_profiles();
    app.screen = Screen::Profiles {
        target,
        editor: None,
        status: Some(format!("Saved “{name}”.{secret_note}")),
    };
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Synchronously open a read-only connection with `form`'s details and report
/// success or the failure reason. Mirrors the source screen's blocking connect.
fn test_connection(app: &ReadactusApp, form: &DbForm) -> String {
    let config = match form.to_config() {
        Ok(c) => c,
        Err(e) => return e,
    };
    let engine = form.engine;
    let rt = Arc::clone(&app.tokio_rt);
    match rt.block_on(async { connect_source(engine, config).await }) {
        Ok(_) => "Connection succeeded.".to_string(),
        Err(e) => format!("Connection failed: {e}"),
    }
}

fn engine_label(engine: Engine) -> &'static str {
    match engine {
        Engine::Postgres => "PostgreSQL",
        Engine::MySql => "MySQL / MariaDB",
    }
}

/// A two-option engine picker, styled like the one on the home screen. The
/// port field below is left to the user (it's rarely the engine default when
/// editing a real connection).
fn engine_combo(ui: &mut egui::Ui, engine: &mut Engine) {
    let mut idx = match engine {
        Engine::Postgres => 0,
        Engine::MySql => 1,
    };
    egui::ComboBox::from_id_salt("profile_engine")
        .width(ui.available_width())
        .show_index(ui, &mut idx, 2, |i| engine_label(index_engine(i)).to_string());
    *engine = index_engine(idx);
}

fn index_engine(i: usize) -> Engine {
    match i {
        0 => Engine::Postgres,
        _ => Engine::MySql,
    }
}
