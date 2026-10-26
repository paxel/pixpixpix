//! Seam 3: the settings file.

use pixpixpix::settings::{self, Settings};

#[test]
fn missing_file_gives_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let loaded = settings::load(&dir.path().join("settings.toml"));
    assert_eq!(loaded, Ok(Settings::default()));
    assert_eq!(Settings::default().cache_per_side, 5);
}

#[test]
fn saved_settings_are_loaded_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config/settings.toml");
    let saved = Settings { cache_per_side: 12 };
    settings::save(&path, &saved).unwrap();
    assert_eq!(settings::load(&path), Ok(saved));
}

#[test]
fn hand_edited_file_is_read_with_missing_and_unknown_fields_tolerated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    std::fs::write(&path, "# mine\ncache_per_side = 3\nfuture = \"x\"\n").unwrap();
    assert_eq!(settings::load(&path), Ok(Settings { cache_per_side: 3 }));
    std::fs::write(&path, "future = 1\n").unwrap();
    assert_eq!(settings::load(&path), Ok(Settings::default()));
}

#[test]
fn out_of_range_cache_size_is_clamped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    std::fs::write(&path, "cache_per_side = 100\n").unwrap();
    assert_eq!(settings::load(&path), Ok(Settings { cache_per_side: 20 }));
    std::fs::write(&path, "cache_per_side = 0\n").unwrap();
    assert_eq!(settings::load(&path), Ok(Settings { cache_per_side: 1 }));
}

#[test]
fn broken_file_is_an_error_naming_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    std::fs::write(&path, "cache_per_side = \"many\"\n").unwrap();
    let error = settings::load(&path).unwrap_err();
    assert!(error.contains("settings.toml"), "{error}");
    assert!(error.contains("cache_per_side"), "{error}");
}

#[test]
fn unwritable_place_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("file");
    std::fs::write(&blocker, "").unwrap();
    let error = settings::save(&blocker.join("settings.toml"), &Settings::default()).unwrap_err();
    assert!(error.starts_with("Cannot save"), "{error}");
}
