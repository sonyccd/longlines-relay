//! The relay's settings: a small versioned JSON file. Never holds the token.
//!
//! Unknown fields are ignored and missing ones take their defaults, so an older relay can read a
//! newer file and the other way round. Phase 2 adds `telnet_port` as one more field.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The version this build writes.
pub const SETTINGS_VERSION: u32 = 1;
/// N1MM Logger+'s default contact broadcast port.
pub const DEFAULT_PORT: u16 = 12060;
/// The file name inside the app's config folder.
pub const SETTINGS_FILE: &str = "settings.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    /// The UDP port to listen on.
    pub port: u16,
    /// "Start when I log in" (default on).
    pub autostart: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            port: DEFAULT_PORT,
            autostart: true,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("settings file: {0}")]
    Io(#[from] std::io::Error),
    #[error("settings file is not valid: {0}")]
    Json(#[from] serde_json::Error),
}

impl Settings {
    /// Reads the file. A missing file gives the defaults.
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        let mut settings: Settings = serde_json::from_str(&text)?;
        settings.migrate();
        Ok(settings)
    }

    /// Reads the file; on any problem logs it and returns the defaults.
    pub fn load_or_default(path: &Path) -> Self {
        Self::load(path).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "using default settings");
            Self::default()
        })
    }

    /// Writes the file atomically (a temporary file, then a rename).
    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut current = self.clone();
        current.version = SETTINGS_VERSION;
        let text = serde_json::to_string_pretty(&current)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Brings an older (or damaged) file's values up to this version.
    fn migrate(&mut self) {
        if self.port == 0 {
            self.port = DEFAULT_PORT;
        }
        // Version 1 is the first; later versions add their steps here.
        self.version = self.version.max(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let s = Settings::load(&dir.path().join(SETTINGS_FILE)).unwrap();
        assert_eq!(s, Settings::default());
        assert_eq!(s.port, 12060);
        assert!(s.autostart);
    }

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join(SETTINGS_FILE);
        let s = Settings {
            port: 12061,
            autostart: false,
            ..Settings::default()
        };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), s);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"version\": 1"));
    }

    #[test]
    fn tolerates_unknown_and_missing_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);
        std::fs::write(
            &path,
            r#"{"version": 2, "telnet_port": 7300, "port": 12062}"#,
        )
        .unwrap();
        let s = Settings::load(&path).unwrap();
        assert_eq!(s.port, 12062);
        assert!(s.autostart);
        assert_eq!(s.version, 2);
        std::fs::write(&path, r#"{"port": 0}"#).unwrap();
        assert_eq!(Settings::load(&path).unwrap().port, DEFAULT_PORT);
    }

    #[test]
    fn damaged_file_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);
        std::fs::write(&path, "{not json").unwrap();
        assert!(Settings::load(&path).is_err());
        assert_eq!(Settings::load_or_default(&path), Settings::default());
    }
}
