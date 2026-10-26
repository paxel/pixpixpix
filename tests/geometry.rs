//! Seam 3: the window geometry file.

use pixpixpix::geometry::{self, Geometry};

#[test]
fn saved_geometry_is_loaded_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state/window.toml");
    let saved = Geometry {
        width: 900.0,
        height: 700.0,
        x: Some(10.0),
        y: None,
    };
    geometry::save(&path, &saved).unwrap();
    assert_eq!(geometry::load(&path), Some(saved));
}

#[test]
fn missing_or_garbage_or_tiny_geometry_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("window.toml");
    assert_eq!(geometry::load(&path), None);
    std::fs::write(&path, "not toml at all [").unwrap();
    assert_eq!(geometry::load(&path), None);
    std::fs::write(&path, "width = 1.0\nheight = 1.0\n").unwrap();
    assert_eq!(geometry::load(&path), None);
}
