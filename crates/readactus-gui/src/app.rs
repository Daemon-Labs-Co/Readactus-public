use std::sync::mpsc;
use std::sync::Arc;

use ddbcore::{Catalog, ConnectionConfig};
use eframe::egui;
use readactus_core::{CopyReport, Engine, Plan};
use readactus_detect::Finding;
use readactus_license::{current_entitlements, Entitlements, ISSUER_PUBLIC_KEY_B32};

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
    Table { schema: String, table: String, rows: u64 },
    Done(CopyReport),
    Failed(String),
}

pub enum Screen {
    Activate {
        key_input: String,
        error: Option<String>,
    },
    Home,
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
    pub tokio_rt: Arc<tokio::runtime::Runtime>,
}

impl ReadactusApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
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
            tokio_rt,
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
        match &self.screen {
            Screen::Activate { .. } => screens::activate::show(self, ui),
            Screen::Home => screens::home::show(self, ui),
            Screen::SourceConnection { .. } => screens::source::show(self, ui),
            Screen::PlanReview { .. } => screens::review::show(self, ui),
            Screen::TargetConnection { .. } => screens::target::show(self, ui),
            Screen::CopyProgress { .. } => screens::progress::show(self, ui),
            Screen::Results { .. } => screens::result::show(self, ui),
        }
    }
}
