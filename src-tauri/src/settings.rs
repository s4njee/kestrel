//! settings.rs — Persisted user settings.
//!
//! A single versioned `settings.json` under the app config dir holding
//! preferences that survive restarts: transfer concurrency, the default conflict
//! resolution, an optional default local directory, and whether to show hidden
//! (dot) files. The store rewrites the file atomically on every change.
//!
//! Runtime application (pushing concurrency and the conflict policy into the
//! engine) lives in `commands/settings.rs`; this module only owns persistence.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// On-disk schema version for `settings.json`.
const SCHEMA_VERSION: u32 = 1;

/// The lowest and highest transfer concurrency we allow.
const MIN_CONCURRENCY: u8 = 1;
const MAX_CONCURRENCY: u8 = 8;

/// User-configurable settings (also the webview DTO; camelCase).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Max concurrently-running transfers (clamped to 1..=8).
    pub concurrency: u8,
    /// Default conflict resolution: "ask" | "overwrite" | "skip" | "rename" |
    /// "resume".
    pub default_conflict: String,
    /// Directory the local pane opens on launch (home dir when `None`).
    #[serde(default)]
    pub default_local_dir: Option<String>,
    /// Whether hidden (dot) files are shown in the panes.
    #[serde(default)]
    pub show_hidden: bool,
}

impl Default for Settings {
    /// Sensible first-run defaults: 3 concurrent transfers, prompt on conflict,
    /// no pinned local dir, hidden files off.
    ///
    /// Returns: the default [`Settings`].
    fn default() -> Self {
        Settings {
            concurrency: 3,
            default_conflict: "ask".to_string(),
            default_local_dir: None,
            show_hidden: false,
        }
    }
}

impl Settings {
    /// Clamp out-of-range fields to their valid bounds.
    ///
    /// Arguments: `&mut self`.
    /// Returns: `()`. Currently bounds `concurrency` into 1..=8; unknown
    /// `default_conflict` strings are tolerated (the command layer maps unknown
    /// values to "ask").
    fn normalize(&mut self) {
        self.concurrency = self.concurrency.clamp(MIN_CONCURRENCY, MAX_CONCURRENCY);
    }
}

/// The `settings.json` envelope: a version tag plus the settings object.
#[derive(Debug, Serialize, Deserialize)]
struct SettingsFile {
    version: u32,
    settings: Settings,
}

/// In-memory settings backed by an atomically-rewritten JSON file.
pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    /// Load settings from `path`, falling back to defaults when missing or
    /// unreadable (a bad file is left untouched until the next save).
    ///
    /// Arguments: `path` — the `settings.json` location.
    /// Returns: the loaded [`SettingsStore`].
    pub fn load(path: PathBuf) -> Self {
        let mut settings = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<SettingsFile>(&bytes) {
                Ok(file) if file.version == SCHEMA_VERSION => file.settings,
                Ok(file) => {
                    tracing::warn!(version = file.version, "unsupported settings.json version");
                    Settings::default()
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to parse settings.json; using defaults");
                    Settings::default()
                }
            },
            Err(_) => Settings::default(),
        };
        settings.normalize();
        SettingsStore {
            path,
            current: Mutex::new(settings),
        }
    }

    /// Snapshot the current settings.
    ///
    /// Returns: a clone of the settings.
    pub fn get(&self) -> Settings {
        self.current.lock().expect("settings mutex poisoned").clone()
    }

    /// Replace and persist the settings (normalizing first).
    ///
    /// Arguments: `settings` — the new settings to store.
    /// Returns: the stored (normalized) settings.
    pub fn save(&self, mut settings: Settings) -> Settings {
        settings.normalize();
        *self.current.lock().expect("settings mutex poisoned") = settings.clone();
        self.persist(&settings);
        settings
    }

    /// Rewrite `settings.json` atomically (temp file + rename). Failures are
    /// logged rather than propagated. Returns: `()`.
    ///
    /// Arguments: `settings` — the settings to serialize into the file.
    fn persist(&self, settings: &Settings) {
        let file = SettingsFile {
            version: SCHEMA_VERSION,
            settings: settings.clone(),
        };
        let json = match serde_json::to_vec_pretty(&file) {
            Ok(json) => json,
            Err(e) => {
                tracing::warn!(error = %e, "failed to serialize settings");
                return;
            }
        };
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = self.path.with_extension("json.tmp");
        if let Err(e) = std::fs::write(&tmp, &json).and_then(|()| std::fs::rename(&tmp, &self.path)) {
            tracing::warn!(error = %e, "failed to write settings.json");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A missing settings file yields the defaults.
    #[test]
    fn missing_file_yields_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(dir.path().join("settings.json"));
        assert_eq!(store.get(), Settings::default());
    }

    /// Saved settings round-trip from disk and record the schema version.
    #[test]
    fn save_round_trips_and_records_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let store = SettingsStore::load(path.clone());
        let saved = store.save(Settings {
            concurrency: 6,
            default_conflict: "overwrite".into(),
            default_local_dir: Some("/tmp".into()),
            show_hidden: true,
        });
        assert_eq!(saved.concurrency, 6);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"version\": 1"));
        // Reloading sees the same settings.
        assert_eq!(SettingsStore::load(path).get(), saved);
    }

    /// Concurrency is clamped into 1..=8 on save.
    #[test]
    fn concurrency_is_clamped() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(dir.path().join("settings.json"));
        assert_eq!(store.save(Settings { concurrency: 99, ..Settings::default() }).concurrency, 8);
        assert_eq!(store.save(Settings { concurrency: 0, ..Settings::default() }).concurrency, 1);
    }

    /// A malformed settings file falls back to the defaults.
    #[test]
    fn malformed_file_falls_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"{ not valid").unwrap();
        assert_eq!(SettingsStore::load(path).get(), Settings::default());
    }
}
