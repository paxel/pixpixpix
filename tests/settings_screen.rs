//! Seam 1: the app core's settings: the `s` screen, loading after the first
//! frame, and applying and saving changes.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Key, KeyPress, Screen};
use pixpixpix::decode::DecodedImage;
use pixpixpix::settings::Settings;

fn image() -> DecodedImage {
    DecodedImage {
        width: 1,
        height: 1,
        rgba: Arc::from(vec![0; 4]),
    }
}

fn p(name: &str) -> PathBuf {
    PathBuf::from(format!("/p/{name}.jpg"))
}

fn press(core: &mut Core, key: Key) -> Vec<Command> {
    core.handle(Event::Key(KeyPress::plain(key)))
}

fn decoded(core: &mut Core, name: &str) -> Vec<Command> {
    core.handle(Event::Decoded {
        path: p(name),
        result: Ok(image()),
    })
}

fn band(core: &Core) -> Vec<PathBuf> {
    core.view().band.iter().map(|t| t.path.clone()).collect()
}

fn key_names(core: &Core) -> Vec<&'static str> {
    core.view().key_line.keys.iter().map(|k| k.keys).collect()
}

const QUIT: &str = if cfg!(target_os = "macos") {
    "Cmd+Q"
} else {
    "Ctrl+Q"
};

/// Showing `f` of `a` … `k`, with `e` and `g` decoded and the other
/// neighbours still decoding.
fn browsing() -> Core {
    let (mut core, _) = Core::start(Ok(p("f")));
    decoded(&mut core, "f");
    core.handle(Event::FirstFrameShown);
    let files = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k"];
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(files.iter().map(|f| p(f)).collect()),
    });
    decoded(&mut core, "e");
    decoded(&mut core, "g");
    core
}

#[test]
fn settings_are_read_only_after_the_first_frame() {
    let (mut core, commands) = Core::start(Ok(p("f")));
    assert!(!commands.contains(&Command::LoadSettings));
    decoded(&mut core, "f");
    let commands = core.handle(Event::FirstFrameShown);
    assert_eq!(commands.first(), Some(&Command::LoadSettings));
    assert_eq!(core.view().settings, Settings::default());
}

#[test]
fn s_opens_and_closes_the_settings_screen() {
    let mut core = browsing();
    assert!(!core.view().settings_open);
    // Opening reads the default apps for their section.
    assert!(matches!(
        press(&mut core, Key::S).as_slice(),
        [Command::ReadDefaults { .. }]
    ));
    assert!(core.view().settings_open);
    assert_eq!(key_names(&core), ["S/Esc", QUIT]);
    press(&mut core, Key::S);
    assert!(!core.view().settings_open);
    assert_eq!(
        key_names(&core),
        [
            "Left/Right",
            "+/-",
            "0",
            "1",
            "L/R",
            "Enter",
            "C/Shift+C",
            "O",
            "I",
            "X",
            "P",
            "G",
            "N",
            "B",
            "Tab",
            "S",
            "F",
            QUIT
        ]
    );
}

#[test]
fn escape_closes_the_settings_screen() {
    let mut core = browsing();
    press(&mut core, Key::S);
    press(&mut core, Key::Escape);
    assert!(!core.view().settings_open);
}

#[test]
fn the_open_screen_takes_every_key_but_quit() {
    let mut core = browsing();
    press(&mut core, Key::S);
    for key in [Key::Right, Key::Left, Key::N, Key::B, Key::Tab, Key::F] {
        assert!(press(&mut core, key).is_empty(), "{key:?}");
    }
    assert!(matches!(&core.view().screen, Screen::Image { path, .. } if *path == p("f")));
    assert!(core.view().dir_column.is_none());
    assert!(!core.view().fullscreen);
    assert!(core.view().settings_open);
    assert_eq!(
        core.handle(Event::Key(KeyPress::command(Key::Q))),
        [Command::Quit]
    );
}

#[test]
fn loaded_cache_size_re_plans_the_window() {
    let mut core = browsing();
    let commands = core.handle(Event::SettingsLoaded(Ok(Settings {
        cache_per_side: 1,
        ..Settings::default()
    })));
    let cancelled: Vec<Command> = ["a", "b", "c", "d", "h", "i", "j", "k"]
        .iter()
        .map(|n| Command::CancelDecode(p(n)))
        .collect();
    assert_eq!(commands, cancelled);
    assert_eq!(band(&core), [p("e"), p("f"), p("g")]);
    assert_eq!(core.view().settings.cache_per_side, 1);
    assert_eq!(core.view().settings_error, None);
}

#[test]
fn changed_cache_size_takes_effect_and_is_saved() {
    let mut core = browsing();
    core.handle(Event::SettingsLoaded(Ok(Settings {
        cache_per_side: 1,
        ..Settings::default()
    })));
    let commands = core.handle(Event::SettingsChanged(Settings {
        cache_per_side: 2,
        ..Settings::default()
    }));
    assert_eq!(
        commands,
        [
            Command::Decode(p("h")),
            Command::Decode(p("d")),
            Command::SaveSettings(Settings {
                cache_per_side: 2,
                ..Settings::default()
            }),
        ]
    );
    assert_eq!(core.view().settings.cache_per_side, 2);
}

#[test]
fn changed_value_is_clamped_and_an_unchanged_one_is_not_saved() {
    let mut core = browsing();
    let commands = core.handle(Event::SettingsChanged(Settings {
        cache_per_side: 99,
        ..Settings::default()
    }));
    assert_eq!(
        commands.last(),
        Some(&Command::SaveSettings(Settings {
            cache_per_side: 20,
            ..Settings::default()
        }))
    );
    assert!(
        core.handle(Event::SettingsChanged(Settings {
            cache_per_side: 30,
            ..Settings::default()
        }))
        .is_empty()
    );
}

#[test]
fn a_broken_file_keeps_the_defaults_and_reports_why() {
    let mut core = browsing();
    let commands = core.handle(Event::SettingsLoaded(Err("Cannot read x: bad".into())));
    assert!(commands.is_empty());
    assert_eq!(core.view().settings, Settings::default());
    assert_eq!(
        core.view().settings_error.as_deref(),
        Some("Cannot read x: bad")
    );
}

#[test]
fn a_failed_save_is_reported_and_a_good_one_clears_the_error() {
    let mut core = browsing();
    core.handle(Event::SettingsSaved(Err("Cannot save x: full".into())));
    assert_eq!(
        core.view().settings_error.as_deref(),
        Some("Cannot save x: full")
    );
    core.handle(Event::SettingsSaved(Ok(())));
    assert_eq!(core.view().settings_error, None);
}
