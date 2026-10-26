//! The user's settings, kept in a TOML file in the platform's config directory
//! and edited through the settings screen. Read only after the first frame.
//! Missing fields take their defaults and unknown ones are ignored, so files
//! of older and newer versions both load.

use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The neighbours per side the cache may be set to.
pub const CACHE_PER_SIDE: RangeInclusive<usize> = 1..=20;

/// The sizes of overview thumbnails `+`/`-` step through, in points.
pub const THUMBNAIL_SIZES: [u32; 6] = [SMALLEST, 128, 160, 200, 256, LARGEST];
const SMALLEST: u32 = 96;
const LARGEST: u32 = 320;

/// The sides, in pixels, an export may be given.
pub const EXPORT_SIDE: RangeInclusive<u32> = 1..=16384;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// How many neighbours per side of the current file are kept decoded.
    pub cache_per_side: usize,
    /// The program images are opened with by `o`; empty for the system
    /// default.
    pub image_app: String,
    /// The program videos are opened with by `o`; empty for the system
    /// default.
    pub video_app: String,
    /// The side of an overview thumbnail in points, from 96 to 320.
    pub thumbnail_size: u32,
    /// The width of the last export, in pixels.
    pub export_width: u32,
    /// The height of the last export, in pixels.
    pub export_height: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            cache_per_side: 5,
            image_app: String::new(),
            video_app: String::new(),
            thumbnail_size: 160,
            export_width: 1920,
            export_height: 1080,
        }
    }
}

impl Settings {
    /// These settings with every value moved into its range.
    pub fn clamped(self) -> Self {
        Self {
            cache_per_side: self
                .cache_per_side
                .clamp(*CACHE_PER_SIDE.start(), *CACHE_PER_SIDE.end()),
            thumbnail_size: self.thumbnail_size.clamp(SMALLEST, LARGEST),
            export_width: self
                .export_width
                .clamp(*EXPORT_SIDE.start(), *EXPORT_SIDE.end()),
            export_height: self
                .export_height
                .clamp(*EXPORT_SIDE.start(), *EXPORT_SIDE.end()),
            ..self
        }
    }
}

/// The thumbnail size one step up from `size`, or down; the largest and the
/// smallest stay. A size between steps goes to the step it is next to.
pub fn step_thumbnail_size(size: u32, larger: bool) -> u32 {
    let next = if larger {
        THUMBNAIL_SIZES.iter().find(|s| **s > size)
    } else {
        THUMBNAIL_SIZES.iter().rev().find(|s| **s < size)
    };
    next.copied().unwrap_or(size).clamp(SMALLEST, LARGEST)
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
