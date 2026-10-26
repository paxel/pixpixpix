//! The window's last size and position, kept in a small TOML file in the
//! platform's state directory. Read once before the window opens.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
    pub width: f32,
    pub height: f32,
    /// Absent where the platform does not tell the position (Wayland).
    pub x: Option<f32>,
    pub y: Option<f32>,
}

/// Where the geometry file lives.
pub fn default_path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("io.github", "paxel", "pixpixpix")?;
    let dir = dirs.state_dir().unwrap_or_else(|| dirs.data_local_dir());
    Some(dir.join("window.toml"))
}

/// The stored geometry; `None` when there is none or it cannot be read.
pub fn load(path: &Path) -> Option<Geometry> {
    let text = std::fs::read_to_string(path).ok()?;
    let geometry: Geometry = toml::from_str(&text).ok()?;
    let sane = geometry.width >= 100.0 && geometry.height >= 100.0;
    sane.then_some(geometry)
}

pub fn save(path: &Path, geometry: &Geometry) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = toml::to_string(geometry).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}
