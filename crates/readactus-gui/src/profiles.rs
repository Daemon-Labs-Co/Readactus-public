//! Connection profiles — Readactus' "My Databases".
//!
//! A profile is a named, reusable set of connection details. The non-secret
//! fields are persisted as JSON under the OS config directory (alongside the
//! license `activation.json`); the password is kept out of that file and
//! stored in the OS secret store (macOS Keychain / Windows Credential Manager
//! / Linux Secret Service) via the `keyring` crate, keyed by the profile id.
//!
//! If the secret store is unavailable, profiles still work — the password just
//! isn't remembered and the user re-enters it when they use the profile.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::app::DbForm;
use readactus_core::Engine;

/// The keyring "service" all Readactus profile passwords live under. Each
/// profile's password is stored against this service with the profile id as
/// the account name.
const KEYRING_SERVICE: &str = "readactus";

/// A saved connection, minus the password (which lives in the OS secret store).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    pub engine: Engine,
    pub host: String,
    /// Kept as a string to round-trip cleanly with [`DbForm`], which edits it
    /// as text and only parses to `u16` at connect time.
    pub port: String,
    pub database: String,
    pub username: String,
    #[serde(default)]
    pub use_tls: bool,
}

impl ConnectionProfile {
    /// Build a profile from a connection form plus a user-facing name. A fresh
    /// random id is minted so the password can be filed against it.
    pub fn from_form(name: String, form: &DbForm) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            engine: form.engine,
            host: form.host.clone(),
            port: form.port.clone(),
            database: form.database.clone(),
            username: form.username.clone(),
            use_tls: form.use_tls,
        }
    }

    /// Copy this profile's editable fields onto an existing form (used when
    /// saving edits back over the same id).
    pub fn update_from_form(&mut self, name: String, form: &DbForm) {
        self.name = name;
        self.engine = form.engine;
        self.host = form.host.clone();
        self.port = form.port.clone();
        self.database = form.database.clone();
        self.username = form.username.clone();
        self.use_tls = form.use_tls;
    }

    /// Rebuild a connection form from this profile, filling the password from
    /// the OS secret store (empty if none is stored or the store is
    /// unavailable).
    pub fn to_form(&self) -> DbForm {
        DbForm {
            engine: self.engine,
            host: self.host.clone(),
            port: self.port.clone(),
            database: self.database.clone(),
            username: self.username.clone(),
            password: get_password(&self.id).unwrap_or_default(),
            use_tls: self.use_tls,
        }
    }
}

// ---------------------------------------------------------------------------
// JSON persistence
// ---------------------------------------------------------------------------

/// `~/.config/readactus/connections.json` (or the platform equivalent).
fn store_path() -> Result<PathBuf, String> {
    let dir = dirs::config_dir()
        .ok_or_else(|| "no config directory on this platform".to_string())?
        .join("readactus");
    Ok(dir.join("connections.json"))
}

/// Load all saved profiles. A missing or unreadable/corrupt file yields an
/// empty list rather than an error — a first run simply has no profiles.
pub fn load_profiles() -> Vec<ConnectionProfile> {
    let path = match store_path() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    match serde_json::from_str(&contents) {
        Ok(profiles) => profiles,
        Err(e) => {
            tracing::warn!("ignoring corrupt connections.json: {e}");
            Vec::new()
        }
    }
}

/// Persist the full profile list, creating the config directory if needed.
pub fn save_profiles(profiles: &[ConnectionProfile]) -> Result<(), String> {
    let path = store_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(profiles).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Password storage (OS secret store)
// ---------------------------------------------------------------------------

fn entry(id: &str) -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(KEYRING_SERVICE, id)
}

/// Store (or overwrite) a profile's password in the OS secret store. Returns
/// `Err` with a human-readable reason if the store can't be reached, so the
/// caller can surface "saved, but password not remembered".
pub fn set_password(id: &str, password: &str) -> Result<(), String> {
    entry(id)
        .and_then(|e| e.set_password(password))
        .map_err(|e| e.to_string())
}

/// Fetch a profile's password, or `None` if none is stored / the store is
/// unavailable.
pub fn get_password(id: &str) -> Option<String> {
    match entry(id).and_then(|e| e.get_password()) {
        Ok(pw) => Some(pw),
        Err(keyring::Error::NoEntry) => None,
        Err(e) => {
            tracing::warn!("could not read password from secret store: {e}");
            None
        }
    }
}

/// Remove a profile's password from the OS secret store. A missing entry is
/// treated as success (the desired end state — no stored password — holds).
pub fn delete_password(id: &str) {
    match entry(id).and_then(|e| e.delete_credential()) {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(e) => tracing::warn!("could not delete password from secret store: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_form() -> DbForm {
        DbForm {
            engine: Engine::Postgres,
            host: "db.internal".into(),
            port: "5433".into(),
            database: "orders".into(),
            username: "readonly".into(),
            password: "s3cret".into(),
            use_tls: true,
        }
    }

    #[test]
    fn from_form_copies_fields_and_mints_id() {
        let p = ConnectionProfile::from_form("Staging".into(), &sample_form());
        assert_eq!(p.name, "Staging");
        assert_eq!(p.host, "db.internal");
        assert_eq!(p.port, "5433");
        assert_eq!(p.database, "orders");
        assert_eq!(p.username, "readonly");
        assert!(p.use_tls);
        assert!(!p.id.is_empty());
    }

    #[test]
    fn password_is_never_serialized() {
        let p = ConnectionProfile::from_form("Staging".into(), &sample_form());
        let json = serde_json::to_string(&[p]).unwrap();
        assert!(!json.contains("password"));
        assert!(!json.contains("s3cret"));
    }

    #[test]
    fn json_round_trips() {
        let p = ConnectionProfile::from_form("Staging".into(), &sample_form());
        let json = serde_json::to_string(&[p.clone()]).unwrap();
        let back: Vec<ConnectionProfile> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].id, p.id);
        assert_eq!(back[0].host, p.host);
        assert_eq!(back[0].use_tls, p.use_tls);
    }

    #[test]
    fn use_tls_defaults_when_absent() {
        // Profiles written before `use_tls` existed must still load.
        let json = r#"[{"id":"x","name":"Old","engine":"Postgres",
            "host":"h","port":"5432","database":"d","username":"u"}]"#;
        let back: Vec<ConnectionProfile> = serde_json::from_str(json).unwrap();
        assert!(!back[0].use_tls);
    }

    #[test]
    fn update_from_form_overwrites_editable_fields_but_keeps_id() {
        let mut p = ConnectionProfile::from_form("Old".into(), &sample_form());
        let id = p.id.clone();
        let mut form = sample_form();
        form.host = "new.host".into();
        form.use_tls = false;
        p.update_from_form("New".into(), &form);
        assert_eq!(p.id, id);
        assert_eq!(p.name, "New");
        assert_eq!(p.host, "new.host");
        assert!(!p.use_tls);
    }
}
