//! The user's settings, kept in a TOML file in the platform's config directory
//! and edited through the settings screen. Read only after the first frame.
//! Missing fields take their defaults and unknown ones are ignored, so files
//! of older and newer versions both load.

use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The neighbours per side the cache may be set to.
pub const CACHE_PER_SIDE: RangeInclusive<usize> = 1..=20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// How many neighbours per side of the current file are kept decoded.
    pub cache_per_side: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self { cache_per_side: 5 }
    }
}

impl Settings {
    /// These settings with every value moved into its range.
    pub fn clamped(self) -> Self {
        Self {
            cache_per_side: self
                .cache_per_side
                .clamp(*CACHE_PER_SIDE.start(), *CACHE_PER_SIDE.end()),
        }
    }
}

/// Where the settings file lives.
pub fn default_path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("io.github", "paxel", "pixpixpix")?;
    Some(dirs.config_dir().join("settings.toml"))
}

/// The stored settings, the defaults when there is no file yet, or why the
/// file cannot be used.
pub fn load(path: &Path) -> Result<Settings, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
        Err(e) => return Err(format!("Cannot read {}: {e}", path.display())),
    };
    toml::from_str::<Settings>(&text)
        .map(Settings::clamped)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    let write = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string(settings).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    };
    write().map_err(|e| format!("Cannot save {}: {e}", path.display()))
}
