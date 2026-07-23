use std::sync::mpsc;
use std::sync::Arc;

use ddbcore::{Catalog, ConnectionConfig};
use eframe::egui;
use readactus_core::{CopyReport, Engine, Plan};
use readactus_detect::Finding;
use readactus_license::{current_entitlements, Entitlements, ISSUER_PUBLIC_KEY_B32};

use crate::profiles::ConnectionProfile;
use crate::screens;

#[derive(Debug, Clone)]
pub struct DbForm {
    pub engine: Engine,
    pub host: String,
    pub port: String,
    pub database: String,
    pub username: String,
    pub password: String,
    pub use_tls: bool,
}

impl Default for DbForm {
    fn default() -> Self {
        Self {
            engine: Engine::Postgres,
            host: "localhost".into(),
            port: "5432".into(),
            database: String::new(),
            username: String::new(),
            password: String::new(),
            use_tls: false,
        }
    }
}

impl DbForm {
    /// A blank form for `engine`, seeded with that engine's default port.
    pub fn for_engine(engine: Engine) -> Self {
        let port = match engine {
            Engine::Postgres => "5432",
            Engine::MySql => "3306",
        };
        Self {
            engine,
            port: port.into(),
            ..Self::default()
        }
    }

    pub fn to_config(&self) -> Result<ConnectionConfig, String> {
        let port: u16 = self.port.parse().map_err(|_| "invalid port number")?;
        Ok(ConnectionConfig {
            host: self.host.clone(),
            port,
            database: self.database.clone(),
            username: self.username.clone(),
            password: self.password.clone(),
            encryption: if self.use_tls {
                ddbcore::EncryptionMode::Tls { verify_cert: true }
            } else {
                ddbcore::EncryptionMode::ClearText
            },
            read_only: false,
        })
    }
}

#[derive(Debug, Clone)]
pub enum CopyProgress {
    // Reserved for live per-table progress. `run_copy` currently reports only
    // terminal Done/Failed, so nothing constructs this yet; the progress screen
    // already renders it for when copy gains streaming updates.
    #[allow(dead_code)]
    Table { schema: String, table: String, rows: u64 },
    Done(CopyReport),
    Failed(String),
}

/// Which step of the pipeline a profile picker is feeding. Profiles are a
/// single shared pool (like DataGrip's "My Databases"); this only decides where
/// the chosen connection is used and which screen we return to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnTarget {
    Source,
    Target,
}

/// Transient state for the add/edit form on the "My Databases" screen.
pub struct ProfileEditor {
    /// User-facing profile name.
    pub name: String,
    /// The connection fields being edited.
    pub form: DbForm,
    /// `Some(id)` when editing an existing profile in place; `None` for a new
    /// one.
    pub editing_id: Option<String>,
    /// Validation / test-connection feedback shown inside the editor.
    pub error: Option<String>,
    /// Set while a "Test connection" is running.
    pub testing: bool,
}

pub enum Screen {
    Activate {
        key_input: String,
        error: Option<String>,
    },
    Home,
    /// The "My Databases" manager: pick a saved profile for `target`, or
    /// add/edit/delete/test profiles. `editor` is `Some` while adding/editing.
    Profiles {
        target: ConnTarget,
        editor: Option<ProfileEditor>,
        status: Option<String>,
    },
    SourceConnection {
        form: DbForm,
        error: Option<String>,
        connecting: bool,
    },
    PlanReview {
        threshold: f32,
    },
    TargetConnection {
        form: DbForm,
        error: Option<String>,
        connecting: bool,
    },
    CopyProgress {
        progress: Vec<CopyProgress>,
        rx: mpsc::Receiver<CopyProgress>,
    },
    Results {
        report: CopyReport,
    },
}

pub struct PipelineState {
    pub catalog: Catalog,
    pub plan: Plan,
    pub findings: Vec<Finding>,
}

pub struct ReadactusApp {
    pub screen: Screen,
    pub entitlements: Entitlements,
    pub source_form: DbForm,
    pub source_engine: Engine,
    pub pipeline: Option<PipelineState>,
    /// The shared pool of saved connection profiles ("My Databases"), loaded
    /// once at startup and kept in sync with `connections.json` on every edit.
    pub profiles: Vec<ConnectionProfile>,
    pub tokio_rt: Arc<tokio::runtime::Runtime>,
}

impl ReadactusApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Install the Daemon Labs brand theme: fonts, type scale, spacing and
        // hand-tuned light/dark palettes. This also follows the OS appearance.
        crate::theme::install(&cc.egui_ctx);

        let entitlements = match current_entitlements(ISSUER_PUBLIC_KEY_B32, None) {
            Ok(ent) if ent.registered => ent,
            Ok(_) => Entitlements::free(),
            Err(e) => {
                tracing::warn!("license check failed: {e}");
                Entitlements::free()
            }
        };

        let screen = if entitlements.registered {
            Screen::Home
        } else {
            Screen::Activate {
                key_input: String::new(),
                error: None,
            }
        };

        let tokio_rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to create tokio runtime"),
        );

        Self {
            screen,
            entitlements,
            source_form: DbForm::default(),
            source_engine: Engine::Postgres,
            pipeline: None,
            profiles: crate::profiles::load_profiles(),
            tokio_rt,
        }
    }

    /// Persist the current profile pool, logging (not surfacing) any I/O error
    /// — a failed write shouldn't block the user mid-flow.
    pub fn save_profiles(&self) {
        if let Err(e) = crate::profiles::save_profiles(&self.profiles) {
            tracing::warn!("failed to save connection profiles: {e}");
        }
    }

    pub fn reload_entitlements(&mut self) {
        match current_entitlements(ISSUER_PUBLIC_KEY_B32, None) {
            Ok(ent) => self.entitlements = ent,
            Err(e) => tracing::warn!("license reload failed: {e}"),
        }
    }
}

impl eframe::App for ReadactusApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // The `App::ui` hook hands us a Ui with no margin or background, so
        // the only thing painting behind us would be eframe's fixed dark
        // `clear_color`. Wrap everything in a CentralPanel so the window is
        // filled with the theme's own background (light on the light OS
        // appearance, dark on the dark one) and text keeps proper contrast.
        egui::CentralPanel::default().show(ui, |ui| {
            crate::theme::page(ui, |ui| match &self.screen {
                Screen::Activate { .. } => screens::activate::show(self, ui),
                Screen::Home => screens::home::show(self, ui),
                Screen::Profiles { .. } => screens::profiles::show(self, ui),
                Screen::SourceConnection { .. } => screens::source::show(self, ui),
                Screen::PlanReview { .. } => screens::review::show(self, ui),
                Screen::TargetConnection { .. } => screens::target::show(self, ui),
                Screen::CopyProgress { .. } => screens::progress::show(self, ui),
                Screen::Results { .. } => screens::result::show(self, ui),
            });
        });
    }
}
