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
    let saved = Settings {
        cache_per_side: 12,
        ..Settings::default()
    };
    settings::save(&path, &saved).unwrap();
    assert_eq!(settings::load(&path), Ok(saved));
}

#[test]
fn hand_edited_file_is_read_with_missing_and_unknown_fields_tolerated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    std::fs::write(&path, "# mine\ncache_per_side = 3\nfuture = \"x\"\n").unwrap();
    assert_eq!(
        settings::load(&path),
        Ok(Settings {
            cache_per_side: 3,
            ..Settings::default()
        })
    );
    std::fs::write(&path, "future = 1\n").unwrap();
    assert_eq!(settings::load(&path), Ok(Settings::default()));
}

#[test]
fn out_of_range_cache_size_is_clamped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    std::fs::write(&path, "cache_per_side = 100\n").unwrap();
    assert_eq!(
        settings::load(&path),
        Ok(Settings {
            cache_per_side: 20,
            ..Settings::default()
        })
    );
    std::fs::write(&path, "cache_per_side = 0\n").unwrap();
    assert_eq!(
        settings::load(&path),
        Ok(Settings {
            cache_per_side: 1,
            ..Settings::default()
        })
    );
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

#[test]
fn external_apps_are_kept_and_empty_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    assert_eq!(Settings::default().image_app, "");
    assert_eq!(Settings::default().video_app, "");
    std::fs::write(&path, "image_app = \"gwenview\"\nvideo_app = \"vlc \"\n").unwrap();
    let loaded = settings::load(&path).unwrap();
    assert_eq!(loaded.image_app, "gwenview");
    assert_eq!(loaded.video_app, "vlc ");
    settings::save(&path, &loaded).unwrap();
    assert_eq!(settings::load(&path), Ok(loaded));
}

#[test]
fn thumbnail_size_defaults_to_160_and_is_clamped() {
    assert_eq!(Settings::default().thumbnail_size, 160);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    std::fs::write(&path, "thumbnail_size = 1000\n").unwrap();
    assert_eq!(settings::load(&path).unwrap().thumbnail_size, 320);
    std::fs::write(&path, "thumbnail_size = 10\n").unwrap();
    assert_eq!(settings::load(&path).unwrap().thumbnail_size, 96);
}

#[test]
fn thumbnail_size_steps_through_its_sizes_and_stops_at_the_ends() {
    assert_eq!(settings::step_thumbnail_size(160, true), 200);
    assert_eq!(settings::step_thumbnail_size(160, false), 128);
    assert_eq!(settings::step_thumbnail_size(320, true), 320);
    assert_eq!(settings::step_thumbnail_size(96, false), 96);
    // A hand-edited size between steps goes to the step next to it.
    assert_eq!(settings::step_thumbnail_size(170, true), 200);
    assert_eq!(settings::step_thumbnail_size(170, false), 160);
}

#[test]
fn remembered_default_apps_and_hidden_parts_load_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let saved = Settings {
        earlier_defaults: [
            (
                "HEIC/HEIF".to_owned(),
                "org.kde.gwenview.desktop".to_owned(),
            ),
            ("MPEG-TS".to_owned(), "vlc.desktop".to_owned()),
        ]
        .into(),
        hints_hidden: true,
        band_hidden: true,
        ..Settings::default()
    };
    settings::save(&path, &saved).unwrap();
    assert_eq!(settings::load(&path), Ok(saved));
}
