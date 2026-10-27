//! Seam 2: the egui layer over the core, headless.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pixpixpix::core::{Command, Core, Event, Scale, Screen, VideoStep};
use pixpixpix::decode::DecodedImage;
use pixpixpix::settings::Settings;
use pixpixpix::startup::NoFile;
use pixpixpix::ui;

struct State {
    core: Core,
    textures: ui::Textures,
    commands: Vec<Command>,
}

fn harness(core: Core) -> Harness<'static, State> {
    let state = State {
        core,
        textures: ui::Textures::default(),
        commands: Vec::new(),
    };
    Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(
            |ui, state: &mut State| {
                for event in ui::input_events(ui.ctx()) {
                    let commands = state.core.handle(event);
                    state.commands.extend(commands);
                }
                for event in ui::show(ui, state.core.view(), &mut state.textures) {
                    let commands = state.core.handle(event);
                    state.commands.extend(commands);
                }
            },
            state,
        )
}

fn shown(path: &str) -> Core {
    let (mut core, _) = Core::start(Ok(PathBuf::from(path)));
    core.handle(Event::Decoded {
        path: PathBuf::from(path),
        result: Ok(DecodedImage {
            width: 2,
            height: 1,
            rgba: Arc::from(vec![255; 8]),
        }),
    });
    core
}

#[test]
fn image_is_drawn_with_its_file_name() {
    let mut h = harness(shown("/p/cat.jpg"));
    h.run();
    h.get_by_role_and_label(Role::Image, "cat.jpg");
}

#[test]
fn nothing_to_open_shows_the_message() {
    let (core, _) = Core::start(Err(NoFile {
        dir: PathBuf::from("/x"),
        message: "No images in /x".into(),
    }));
    let mut h = harness(core);
    h.run();
    h.get_by_label("No images in /x");
}

#[test]
fn decode_error_shows_file_name_and_message() {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/bad.jpg")));
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/bad.jpg"),
        result: Err("unexpected end of file".into()),
    });
    let mut h = harness(core);
    h.run();
    h.get_by_label("bad.jpg");
    h.get_by_label("unexpected end of file");
    h.get_by_label(OPEN_HINT);
}

const OPEN_HINT: &str = "Press O to open it in another app";

#[test]
fn nothing_to_open_has_no_open_hint() {
    let (core, _) = Core::start(Err(NoFile {
        dir: PathBuf::from("/x"),
        message: "No images in /x".into(),
    }));
    let mut h = harness(core);
    h.run();
    assert!(h.query_by_label(OPEN_HINT).is_none());
}

#[test]
fn f_key_reaches_the_core() {
    let mut h = harness(shown("/p/cat.jpg"));
    h.run();
    h.key_press(egui::Key::F);
    h.run();
    assert!(h.state().core.view().fullscreen);
    assert_eq!(h.state().commands, [Command::SetFullscreen(true)]);
}

#[test]
fn command_q_reaches_the_core() {
    let mut h = harness(shown("/p/cat.jpg"));
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Q);
    h.run();
    assert_eq!(h.state().commands, [Command::Quit]);
}

/// Shown on `path` with its folder listed as `files`.
fn browsing(path: &str, files: &[&str]) -> Core {
    let mut core = shown(path);
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(files.iter().map(PathBuf::from).collect()),
    });
    core
}

#[test]
fn key_line_lists_the_image_view_keys() {
    let mut h = harness(shown("/p/cat.jpg"));
    h.run();
    h.get_by_label("Left/Right");
    h.get_by_label("+/-");
    h.get_by_label("0");
    h.get_by_label("1");
    h.get_by_label("F");
    h.get_by_label(if cfg!(target_os = "macos") {
        "Cmd+Q"
    } else {
        "Ctrl+Q"
    });
}

#[test]
fn right_arrow_moves_to_the_next_file() {
    let mut h = harness(browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]));
    h.run();
    h.key_press(egui::Key::ArrowRight);
    h.run();
    assert_eq!(
        h.state().core.view().screen,
        Screen::Loading {
            path: PathBuf::from("/p/b.jpg")
        }
    );
}

#[test]
fn ends_show_first_and_last_image_in_the_key_line() {
    let mut h = harness(browsing("/p/a.jpg", &["/p/a.jpg"]));
    h.run();
    h.key_press(egui::Key::ArrowLeft);
    h.run();
    h.get_by_label("first image");
    h.key_press(egui::Key::ArrowRight);
    h.run();
    h.get_by_label("last image");
}

fn decoded(core: &mut Core, path: &str) {
    core.handle(Event::Decoded {
        path: PathBuf::from(path),
        result: Ok(DecodedImage {
            width: 1,
            height: 2,
            rgba: Arc::from(vec![0; 8]),
        }),
    });
}

#[test]
fn band_shows_the_cached_images_with_the_current_one_marked() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg", "/p/c.jpg"]);
    decoded(&mut core, "/p/b.jpg");
    let mut h = harness(core);
    h.run();
    let toggled = |name: &str| {
        h.get_by_role_and_label(Role::Button, name)
            .accesskit_node()
            .toggled()
    };
    assert_eq!(toggled("a.jpg"), Some(Toggled::True));
    assert_eq!(toggled("b.jpg"), Some(Toggled::False));
    assert!(h.query_by_role_and_label(Role::Button, "c.jpg").is_none());
}

#[test]
fn clicking_a_band_thumb_shows_that_image() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]);
    decoded(&mut core, "/p/b.jpg");
    let mut h = harness(core);
    h.run();
    h.get_by_role_and_label(Role::Button, "b.jpg").click();
    h.run();
    assert!(matches!(
        &h.state().core.view().screen,
        Screen::Image { path, .. } if path == Path::new("/p/b.jpg")
    ));
    h.get_by_role_and_label(Role::Image, "b.jpg");
}

#[test]
fn zoom_keys_reach_the_core() {
    let mut h = harness(shown("/p/cat.jpg"));
    h.run();
    h.key_press(egui::Key::Num1);
    h.run();
    assert_eq!(h.state().core.view().zoom.scale, Scale::Exact(1.0));
    h.key_press(egui::Key::Plus);
    h.run();
    assert_eq!(h.state().core.view().zoom.scale, Scale::Exact(1.25));
    h.key_press(egui::Key::Minus);
    h.run();
    assert_eq!(h.state().core.view().zoom.scale, Scale::Exact(1.0));
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Equals);
    h.run();
    assert_eq!(h.state().core.view().zoom.scale, Scale::Exact(1.25));
    h.key_press(egui::Key::Num0);
    h.run();
    assert_eq!(h.state().core.view().zoom.scale, Scale::Fit);
}

#[test]
fn mouse_wheel_over_the_image_zooms_in() {
    let mut h = harness(shown("/p/cat.jpg"));
    h.run();
    h.hover_at(egui::pos2(400.0, 200.0));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 50.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
    // The 2 × 1 image fits at 100 %.
    assert!(matches!(
        h.state().core.view().zoom.scale,
        Scale::Exact(s) if s > 1.0
    ));
}

#[test]
fn dragging_a_zoomed_image_pans_it() {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/big.jpg")));
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/big.jpg"),
        result: Ok(DecodedImage {
            width: 2000,
            height: 1000,
            rgba: Arc::from(vec![0; 2000 * 1000 * 4]),
        }),
    });
    let mut h = harness(core);
    h.run();
    h.key_press(egui::Key::Num1);
    h.run();
    h.drag_at(egui::pos2(400.0, 200.0));
    h.run();
    h.hover_at(egui::pos2(300.0, 200.0));
    h.run();
    h.drop_at(egui::pos2(300.0, 200.0));
    h.run();
    // Dragged left by 100 points: 100 more of the 2000 pixels on the right.
    let zoom = h.state().core.view().zoom;
    assert!((zoom.center_x - 0.55).abs() < 1e-4, "{}", zoom.center_x);
}

fn broken(core: &mut Core, path: &str) {
    core.handle(Event::Decoded {
        path: PathBuf::from(path),
        result: Err("unexpected end of file".into()),
    });
}

#[test]
fn a_broken_file_is_in_the_band_with_an_error_marker() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]);
    broken(&mut core, "/p/b.jpg");
    let mut h = harness(core);
    h.run();
    assert!(h.query_by_role_and_label(Role::Button, "b.jpg").is_none());
    h.get_by_role_and_label(Role::Button, "b.jpg (error)")
        .click();
    h.run();
    h.get_by_label("unexpected end of file");
    h.get_by_label(OPEN_HINT);
    let marked = h
        .get_by_role_and_label(Role::Button, "b.jpg (error)")
        .accesskit_node()
        .toggled();
    assert_eq!(marked, Some(Toggled::True));
}

#[test]
fn an_unreadable_folder_is_shown_in_the_key_line() {
    let mut core = shown("/p/cat.jpg");
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Err("permission denied".into()),
    });
    let mut h = harness(core);
    h.run();
    h.get_by_label("Cannot read the folder: permission denied");
}

/// [`browsing`] with the column opened by `n`: `p` among `q`, and the
/// subfolders `holidays` and `work` of `p` listed.
fn column_open() -> Harness<'static, State> {
    let mut h = harness(browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]));
    h.run();
    h.key_press(egui::Key::N);
    h.run();
    assert_eq!(
        h.state().commands,
        [
            Command::ListDirs(PathBuf::from("/")),
            Command::ListDirs(PathBuf::from("/p"))
        ]
    );
    h.state_mut().core.handle(Event::DirsListed {
        dir: PathBuf::from("/"),
        result: Ok(vec![PathBuf::from("/p"), PathBuf::from("/q")]),
    });
    h.state_mut().core.handle(Event::DirsListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![PathBuf::from("/p/holidays"), PathBuf::from("/p/work")]),
    });
    h.run();
    h
}

fn folder_toggled(h: &Harness<'_, State>, name: &str) -> Option<Toggled> {
    h.get_by_role_and_label(Role::Button, name)
        .accesskit_node()
        .toggled()
}

#[test]
fn n_shows_the_current_folder_among_its_siblings_highlighted() {
    let mut h = column_open();
    assert_eq!(folder_toggled(&h, "p"), Some(Toggled::True));
    assert_eq!(folder_toggled(&h, "q"), Some(Toggled::False));
    // Opening it changed nothing on screen.
    h.get_by_role_and_label(Role::Image, "a.jpg");
    h.key_press(egui::Key::ArrowDown);
    h.run();
    assert_eq!(folder_toggled(&h, "q"), Some(Toggled::True));
    assert_eq!(
        h.state().core.view().screen,
        Screen::Listing {
            dir: PathBuf::from("/q")
        }
    );
    h.key_press(egui::Key::N);
    h.run();
    assert!(h.query_by_label("q").is_none());
}

#[test]
fn enter_goes_into_the_first_subfolder() {
    let mut h = column_open();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(
        h.state().core.view().screen,
        Screen::Listing {
            dir: PathBuf::from("/p/holidays")
        }
    );
    assert_eq!(folder_toggled(&h, "holidays"), Some(Toggled::True));
    assert_eq!(folder_toggled(&h, "work"), Some(Toggled::False));
}

#[test]
fn backspace_goes_to_the_parent() {
    let mut h = column_open();
    h.key_press(egui::Key::Backspace);
    h.run();
    assert_eq!(
        h.state().core.view().screen,
        Screen::Listing {
            dir: PathBuf::from("/")
        }
    );
    assert_eq!(folder_toggled(&h, "/"), Some(Toggled::True));
}

#[test]
fn tab_switches_the_key_line_between_column_and_image_view() {
    let mut h = column_open();
    h.get_by_label("Up/Down");
    assert!(h.query_by_label("Left/Right").is_none());
    h.key_press(egui::Key::Tab);
    h.run();
    h.get_by_label("Left/Right");
    assert!(h.query_by_label("Up/Down").is_none());
}

fn listed(h: &mut Harness<'static, State>, dir: &str, files: Result<Vec<PathBuf>, String>) {
    h.state_mut().core.handle(Event::FolderListed {
        dir: PathBuf::from(dir),
        result: files,
    });
    h.run();
}

#[test]
fn going_down_shows_the_next_folder_with_its_band() {
    let mut h = column_open();
    h.key_press(egui::Key::ArrowDown);
    h.run();
    listed(&mut h, "/q", Ok(vec![PathBuf::from("/q/beach.jpg")]));
    h.state_mut().core.handle(Event::Decoded {
        path: PathBuf::from("/q/beach.jpg"),
        result: Ok(DecodedImage {
            width: 2,
            height: 1,
            rgba: Arc::from(vec![255; 8]),
        }),
    });
    h.run();
    h.get_by_role_and_label(Role::Image, "beach.jpg");
    h.get_by_role_and_label(Role::Button, "beach.jpg");
    assert!(h.query_by_label("a.jpg").is_none());
}

#[test]
fn a_folder_without_images_says_so() {
    let mut h = column_open();
    h.key_press(egui::Key::ArrowDown);
    h.run();
    listed(&mut h, "/q", Ok(Vec::new()));
    h.get_by_label("No images in /q");
}

#[test]
fn entering_an_unreadable_folder_shows_the_error() {
    let mut h = column_open();
    h.key_press(egui::Key::Enter);
    h.run();
    listed(&mut h, "/p/holidays", Err("permission denied".into()));
    h.get_by_label("Cannot read /p/holidays: permission denied");
}

#[test]
fn dot_in_the_column_shows_dot_folders() {
    let mut h = column_open();
    h.state_mut().core.handle(Event::DirsListed {
        dir: PathBuf::from("/"),
        result: Ok(vec![PathBuf::from("/.git"), PathBuf::from("/p")]),
    });
    h.run();
    assert!(h.query_by_label(".git").is_none());
    h.key_press(egui::Key::Period);
    h.run();
    h.get_by_label(".git");
}

#[test]
fn shift_dot_on_any_layout_keeps_dot_folders_and_says_so() {
    let mut h = column_open();
    h.state_mut().core.handle(Event::DirsListed {
        dir: PathBuf::from("/"),
        result: Ok(vec![PathBuf::from("/.git"), PathBuf::from("/p")]),
    });
    h.run();
    // A German layout types `:`, a US one `>`.
    h.event(egui::Event::Key {
        key: egui::Key::Colon,
        physical_key: Some(egui::Key::Period),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::SHIFT,
    });
    h.run();
    h.get_by_label(".git");
    h.get_by_label("dot entries shown, kept");
}

/// Whether the band thumb or overview tile `name` is highlighted for
/// keyboard selection.
fn band_selected(h: &Harness<'_, State>, name: &str) -> Option<bool> {
    h.get_by_role_and_label(Role::Button, name)
        .accesskit_node()
        .is_selected()
}

#[test]
fn b_highlights_the_current_thumb_and_the_arrows_move_the_highlight() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg", "/p/c.jpg"]);
    decoded(&mut core, "/p/b.jpg");
    decoded(&mut core, "/p/c.jpg");
    let mut h = harness(core);
    h.run();
    assert_eq!(band_selected(&h, "a.jpg"), Some(false));
    h.key_press(egui::Key::B);
    h.run();
    assert_eq!(band_selected(&h, "a.jpg"), Some(true));
    h.key_press(egui::Key::ArrowRight);
    h.run();
    h.key_press(egui::Key::ArrowRight);
    h.run();
    assert_eq!(band_selected(&h, "a.jpg"), Some(false));
    assert_eq!(band_selected(&h, "c.jpg"), Some(true));
    // The current mark stays on the shown file.
    let toggled = h
        .get_by_role_and_label(Role::Button, "a.jpg")
        .accesskit_node()
        .toggled();
    assert_eq!(toggled, Some(Toggled::True));
}

#[test]
fn enter_in_the_band_shows_the_highlighted_image_once() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg", "/p/c.jpg"]);
    decoded(&mut core, "/p/b.jpg");
    decoded(&mut core, "/p/c.jpg");
    let mut h = harness(core);
    h.run();
    h.key_press(egui::Key::Tab);
    h.run();
    h.key_press(egui::Key::ArrowRight);
    h.run();
    h.key_press(egui::Key::ArrowRight);
    h.run();
    h.state_mut().commands.clear();
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(matches!(
        &h.state().core.view().screen,
        Screen::Image { path, .. } if path == Path::new("/p/c.jpg")
    ));
    h.get_by_role_and_label(Role::Image, "c.jpg");
    // Every file is cached, and no stray click shows a second one.
    assert_eq!(h.state().commands, []);
    assert_eq!(band_selected(&h, "c.jpg"), Some(false));
    h.get_by_label("Left/Right");
    h.get_by_label("+/-");
}

#[test]
fn the_key_line_lists_the_band_keys_while_it_has_the_focus() {
    let mut h = harness(browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]));
    h.run();
    h.key_press(egui::Key::B);
    h.run();
    h.get_by_label("Enter");
    h.get_by_label("Esc");
    assert!(h.query_by_label("+/-").is_none());
    h.key_press(egui::Key::Escape);
    h.run();
    h.get_by_label("+/-");
    assert!(h.query_by_label("Esc").is_none());
}

const CACHE_LABEL: &str = "Cached images per side";

#[test]
fn s_opens_and_closes_the_settings_screen() {
    let mut h = harness(shown("/p/cat.jpg"));
    h.run();
    assert!(h.query_by_label("Settings").is_none());
    h.key_press(egui::Key::S);
    h.run();
    h.get_by_label("Settings");
    let slider = h.get_by_role_and_label(Role::Slider, CACHE_LABEL);
    assert_eq!(slider.accesskit_node().numeric_value(), Some(5.0));
    h.get_by_label("S/Esc");
    h.key_press(egui::Key::S);
    h.run();
    assert!(h.query_by_label("Settings").is_none());
}

#[test]
fn the_cache_size_is_changed_on_the_settings_screen() {
    let mut h = harness(shown("/p/cat.jpg"));
    h.run();
    h.key_press(egui::Key::S);
    h.run();
    h.state_mut().commands.clear();
    h.get_by_role_and_label(Role::Slider, CACHE_LABEL).focus();
    h.run();
    h.key_press(egui::Key::ArrowRight);
    h.run();
    assert_eq!(h.state().core.view().settings.cache_per_side, 6);
    assert_eq!(
        h.state().commands,
        [Command::SaveSettings(Settings {
            cache_per_side: 6,
            ..Settings::default()
        })]
    );
    let slider = h.get_by_role_and_label(Role::Slider, CACHE_LABEL);
    assert_eq!(slider.accesskit_node().numeric_value(), Some(6.0));
}

#[test]
fn the_settings_screen_shows_why_the_file_is_unusable() {
    let mut core = shown("/p/cat.jpg");
    core.handle(Event::SettingsLoaded(Err(
        "Cannot read settings.toml: bad".into()
    )));
    let mut h = harness(core);
    h.run();
    assert!(h.query_by_label("Cannot read settings.toml: bad").is_none());
    h.key_press(egui::Key::S);
    h.run();
    h.get_by_label("Cannot read settings.toml: bad");
    h.state_mut().commands.clear();
    h.get_by_role_and_label(Role::Button, "Copy").click();
    h.run();
    assert_eq!(
        h.state().commands,
        [Command::CopyReport {
            message: "Cannot read settings.toml: bad".into(),
            path: None,
        }]
    );
}

#[test]
fn o_reaches_the_core_and_a_failure_shows_in_the_key_line() {
    let mut h = harness(shown("/p/cat.jpg"));
    h.run();
    h.get_by_label("O");
    h.key_press(egui::Key::O);
    h.run();
    assert_eq!(
        h.state().commands,
        [Command::OpenExternally {
            path: PathBuf::from("/p/cat.jpg"),
            app: None,
            earlier: None,
        }]
    );
    let commands = h
        .state_mut()
        .core
        .handle(Event::OpenFailed("Cannot start nope: not found".into()));
    assert!(commands.is_empty());
    h.run();
    h.get_by_label("Cannot start nope: not found");
}

#[test]
fn the_external_apps_are_typed_on_the_settings_screen() {
    let mut h = harness(shown("/p/cat.jpg"));
    h.run();
    h.key_press(egui::Key::S);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "App for videos");
    h.get_by_role_and_label(Role::TextInput, "App for images")
        .focus();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "App for images")
        .type_text("gwenview");
    h.run();
    assert_eq!(h.state().core.view().settings.image_app, "gwenview");
    assert_eq!(
        h.state().commands.last(),
        Some(&Command::SaveSettings(Settings {
            image_app: "gwenview".into(),
            ..Settings::default()
        }))
    );
}

/// Showing `/p/wide.jpg`, 200 × 100 pixels.
fn shown_wide() -> Core {
    let path = PathBuf::from("/p/wide.jpg");
    let (mut core, _) = Core::start(Ok(path.clone()));
    core.handle(Event::Decoded {
        path,
        result: Ok(DecodedImage {
            width: 200,
            height: 100,
            rgba: Arc::from(vec![255; 200 * 100 * 4]),
        }),
    });
    core
}

#[test]
fn r_and_l_turn_the_drawn_image() {
    let mut h = harness(shown_wide());
    h.run();
    let size = |h: &Harness<'static, State>| {
        h.get_by_role_and_label(Role::Image, "wide.jpg")
            .rect()
            .size()
    };
    assert_eq!(size(&h), egui::vec2(200.0, 100.0));
    h.key_press(egui::Key::R);
    h.run();
    assert_eq!(size(&h), egui::vec2(100.0, 200.0));
    h.key_press(egui::Key::L);
    h.run();
    assert_eq!(size(&h), egui::vec2(200.0, 100.0));
    h.get_by_label("L/R");
    h.get_by_label("write rotation");
}

#[test]
fn enter_writes_the_rotation_and_a_failure_shows_in_the_key_line() {
    let mut h = harness(shown_wide());
    h.run();
    h.key_press(egui::Key::R);
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(
        h.state().commands.last(),
        Some(&Command::WriteRotation {
            path: PathBuf::from("/p/wide.jpg"),
            rotation: pixpixpix::rotate::Rotation::Right,
        })
    );
    h.state_mut().core.handle(Event::RotationWritten {
        path: PathBuf::from("/p/wide.jpg"),
        result: Err("Cannot rotate wide.jpg: no libturbojpeg".into()),
    });
    h.run();
    h.get_by_label("Cannot rotate wide.jpg: no libturbojpeg");
}

#[test]
fn x_reaches_the_core_and_a_failure_shows_in_the_key_line() {
    let mut h = harness(browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]));
    h.run();
    h.get_by_label("X");
    h.get_by_label("trash");
    h.key_press(egui::Key::X);
    h.run();
    assert_eq!(
        h.state().commands.first(),
        Some(&Command::Trash(PathBuf::from("/p/a.jpg")))
    );
    h.state_mut().core.handle(Event::Trashed {
        path: PathBuf::from("/p/a.jpg"),
        result: Err("Cannot trash a.jpg: Permission denied".into()),
    });
    h.run();
    h.get_by_label("Cannot trash a.jpg: Permission denied");
}

#[test]
fn c_reaches_the_core_and_the_outcome_shows_in_the_key_line() {
    let mut h = harness(browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]));
    h.run();
    h.get_by_label("C/Shift+C");
    h.get_by_label("copy picture/file");
    h.key_press(egui::Key::R);
    h.key_press(egui::Key::C);
    h.run();
    assert!(matches!(
        h.state().commands.last(),
        Some(Command::Copy {
            rotation: pixpixpix::rotate::Rotation::Right,
            ..
        })
    ));
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::C);
    h.run();
    assert_eq!(
        h.state().commands.last(),
        Some(&Command::CopyFile(PathBuf::from("/p/a.jpg")))
    );
    h.state_mut().core.handle(Event::Copied(Ok(())));
    h.run();
    h.get_by_label("copied");
    h.state_mut()
        .core
        .handle(Event::Copied(Err("Cannot copy a.jpg: no display".into())));
    h.run();
    h.get_by_label("Cannot copy a.jpg: no display");
}

/// [`browsing`] on `/p/a.gif` of `a.gif` and `b.gif`, both decoded and
/// animated.
fn browsing_animations() -> Core {
    let mut core = browsing("/p/a.gif", &["/p/a.gif", "/p/b.gif"]);
    core.handle(Event::Animated(PathBuf::from("/p/a.gif")));
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/b.gif"),
        result: Ok(DecodedImage {
            width: 2,
            height: 1,
            rgba: Arc::from(vec![255; 8]),
        }),
    });
    core.handle(Event::Animated(PathBuf::from("/p/b.gif")));
    core
}

#[test]
fn animated_files_are_in_the_band_with_an_animated_marker() {
    let mut h = harness(browsing_animations());
    h.run();
    h.get_by_role_and_label(Role::Button, "a.gif (animated)");
    h.get_by_role_and_label(Role::Button, "b.gif (animated)")
        .click();
    h.run();
    h.get_by_role_and_label(Role::Image, "b.gif");
}

#[test]
fn a_playing_animation_is_drawn_frame_by_frame() {
    let mut core = browsing_animations();
    let frame = |value: u8| pixpixpix::decode::Frame {
        image: DecodedImage {
            width: 2,
            height: 1,
            rgba: Arc::from(vec![value; 8]),
        },
        delay: std::time::Duration::from_millis(100),
    };
    core.handle(Event::FramesDecoded {
        path: PathBuf::from("/p/a.gif"),
        result: Ok(vec![frame(255), frame(10)]),
    });
    let start = std::time::Instant::now();
    core.handle(Event::Tick(start));
    let mut h = harness(core);
    // A playing animation keeps asking for frames, so `run` never settles.
    h.run_steps(3);
    h.get_by_role_and_label(Role::Image, "a.gif");
    h.state_mut()
        .core
        .handle(Event::Tick(start + std::time::Duration::from_millis(150)));
    h.run_steps(3);
    h.get_by_role_and_label(Role::Image, "a.gif");
    h.get_by_role_and_label(Role::Button, "a.gif (animated)");
}

#[test]
fn multi_image_files_show_which_image_and_are_marked_in_the_band() {
    let mut core = browsing("/p/a.tiff", &["/p/a.tiff", "/p/b.jpg"]);
    core.handle(Event::Images {
        path: PathBuf::from("/p/a.tiff"),
        count: 3,
    });
    decoded(&mut core, "/p/b.jpg");
    let mut h = harness(core);
    h.run();
    h.get_by_label("1 / 3");
    h.get_by_role_and_label(Role::Button, "a.tiff (3 images)");
    h.get_by_role_and_label(Role::Button, "b.jpg");
    h.key_press(egui::Key::ArrowDown);
    h.run();
    assert_eq!(
        h.state().commands.last(),
        Some(&Command::DecodeInner {
            path: PathBuf::from("/p/a.tiff"),
            index: 1,
        })
    );
    h.get_by_label("2 / 3");
    h.key_press(egui::Key::ArrowRight);
    h.run();
    assert!(h.query_by_label("2 / 3").is_none());
    assert!(h.query_by_label("1 / 3").is_none());
}

/// [`browsing`] on `/p/a.mp4` of `a.mp4` and `b.jpg`, both decoded.
fn browsing_videos() -> Core {
    let mut core = browsing("/p/a.mp4", &["/p/a.mp4", "/p/b.jpg"]);
    decoded(&mut core, "/p/b.jpg");
    core
}

#[test]
fn a_video_shows_a_play_triangle_and_is_marked_in_the_band() {
    let mut h = harness(browsing_videos());
    h.run();
    h.get_by_role_and_label(Role::Image, "a.mp4");
    h.get_by_role_and_label(Role::Button, "Play");
    h.get_by_role_and_label(Role::Button, "a.mp4 (video)");
    h.get_by_role_and_label(Role::Button, "b.jpg");
    h.get_by_label("Space");
    h.get_by_label("play");
    h.get_by_label("M");
    h.get_by_label("mute");
}

#[test]
fn clicking_play_or_space_plays_and_pauses_the_video() {
    let mut h = harness(browsing_videos());
    h.run();
    h.get_by_role_and_label(Role::Button, "Play").click();
    h.run();
    assert_eq!(
        h.state().commands.last(),
        Some(&Command::PlayVideo {
            path: PathBuf::from("/p/a.mp4"),
            muted: false,
            paused: false,
        })
    );
    assert!(h.query_by_label("Play").is_none());
    h.get_by_label("pause");
    h.key_press(egui::Key::Space);
    h.run();
    assert_eq!(h.state().commands.last(), Some(&Command::PauseVideo));
    h.get_by_role_and_label(Role::Button, "Play");
    h.key_press(egui::Key::M);
    h.run();
    h.get_by_label("unmute");
}

#[test]
fn clicking_a_playing_video_pauses_it() {
    let mut core = browsing_videos();
    core.handle(Event::Clicked);
    let mut h = harness(core);
    h.run();
    h.get_by_role_and_label(Role::Image, "a.mp4").click();
    h.run();
    assert_eq!(h.state().commands.last(), Some(&Command::PauseVideo));
}

#[test]
fn a_video_that_cannot_play_shows_mpvs_error_and_the_open_hint() {
    let mut core = browsing_videos();
    core.handle(Event::Clicked);
    core.handle(Event::VideoFailed {
        path: PathBuf::from("/p/a.mp4"),
        message: "unrecognized file format".into(),
    });
    let mut h = harness(core);
    h.run();
    h.get_by_role_and_label(Role::Image, "a.mp4");
    h.get_by_label("unrecognized file format");
    h.get_by_label("Press O to open it in another app");
    h.get_by_role_and_label(Role::Button, "Play");
}

#[test]
fn dragging_a_zoomed_video_pans_it_without_playing() {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/big.mp4")));
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/big.mp4"),
        result: Ok(DecodedImage {
            width: 2000,
            height: 1000,
            rgba: Arc::from(vec![0; 2000 * 1000 * 4]),
        }),
    });
    let mut h = harness(core);
    h.run();
    h.key_press(egui::Key::Num1);
    h.run();
    h.drag_at(egui::pos2(400.0, 200.0));
    h.run();
    h.hover_at(egui::pos2(300.0, 200.0));
    h.run();
    h.drop_at(egui::pos2(300.0, 200.0));
    h.run();
    let zoom = h.state().core.view().zoom;
    assert!((zoom.center_x - 0.55).abs() < 1e-4, "{}", zoom.center_x);
    assert!(!h.state().core.view().video.as_ref().unwrap().playing);
}

#[test]
fn a_paused_video_shows_step_buttons_that_seek_without_playing() {
    let mut h = harness(browsing_videos());
    h.run();
    for label in ["-30s", "-10s", "-1 frame", "+1 frame", "+10s"] {
        h.get_by_role_and_label(Role::Button, label);
    }
    h.get_by_role_and_label(Role::Button, "+30s").click();
    h.run();
    assert_eq!(
        h.state().commands,
        [
            Command::PlayVideo {
                path: PathBuf::from("/p/a.mp4"),
                muted: false,
                paused: true,
            },
            Command::SeekVideo(VideoStep::Seconds(30)),
        ]
    );
    assert!(!h.state().core.view().video.as_ref().unwrap().playing);
}

#[test]
fn a_playing_video_hides_the_step_buttons() {
    let mut core = browsing_videos();
    core.handle(Event::Clicked);
    let mut h = harness(core);
    h.run();
    assert!(h.query_by_label("+10s").is_none());
    assert!(h.query_by_label("-1 frame").is_none());
}

#[test]
fn modified_arrows_seek_the_video_and_plain_ones_change_the_file() {
    let mut core = browsing_videos();
    core.handle(Event::Clicked);
    let mut h = harness(core);
    h.run();
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowLeft);
    h.run();
    assert_eq!(
        h.state().commands.last(),
        Some(&Command::SeekVideo(VideoStep::Seconds(-10)))
    );
    h.get_by_label("Shift+Left/Right");
    h.get_by_label("Alt+Left/Right");
    h.key_press(egui::Key::ArrowRight);
    h.run();
    h.get_by_role_and_label(Role::Image, "b.jpg");
}

#[test]
fn live_and_motion_photos_are_marked_and_space_plays_their_video() {
    let mut core = browsing("/p/a.heic", &["/p/a.heic", "/p/a.mov", "/p/b.jpg"]);
    decoded(&mut core, "/p/b.jpg");
    core.handle(Event::MotionPhoto(PathBuf::from("/p/b.jpg")));
    let mut h = harness(core);
    h.run();
    h.get_by_label("Live Photo");
    h.get_by_role_and_label(Role::Button, "a.heic (live photo)");
    h.get_by_role_and_label(Role::Button, "b.jpg (motion photo)");
    assert!(
        h.query_by_role_and_label(Role::Button, "a.mov (video)")
            .is_none()
    );
    assert!(h.query_by_label("Play").is_none());
    h.get_by_label("play motion");
    h.key_press(egui::Key::Space);
    h.run();
    assert_eq!(
        h.state().commands.last(),
        Some(&Command::PlayVideo {
            path: PathBuf::from("/p/a.mov"),
            muted: false,
            paused: false,
        })
    );
    h.get_by_label("pause");
    h.key_press(egui::Key::ArrowRight);
    h.run();
    h.get_by_label("Motion Photo");
    assert!(h.query_by_label("Live Photo").is_none());
}

/// [`browsing`] in a harness with the overview opened by `g`; the number of
/// its scan.
fn overview_open() -> (Harness<'static, State>, Option<u64>) {
    let mut h = harness(browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]));
    h.run();
    h.state_mut().commands.clear();
    h.key_press(egui::Key::G);
    h.run();
    let scan = h.state().commands.iter().find_map(|c| match c {
        Command::ScanOverview { scan, .. } => Some(*scan),
        _ => None,
    });
    (h, scan)
}

fn overview_section(h: &mut Harness<'static, State>, scan: u64, dir: &str, files: &[String]) {
    let commands = h.state_mut().core.handle(Event::OverviewSection {
        scan,
        dir: PathBuf::from(dir),
        files: files.iter().map(PathBuf::from).collect(),
    });
    h.state_mut().commands.extend(commands);
    h.run();
}

#[test]
fn g_replaces_image_and_band_by_sections_of_named_placeholders() {
    let (mut h, scan) = overview_open();
    let scan = scan.unwrap();
    h.get_by_label("Scanning…");
    assert!(h.query_by_role_and_label(Role::Image, "a.jpg").is_none());
    assert!(h.query_by_role_and_label(Role::Button, "b.jpg").is_none());
    h.get_by_label("close overview");
    overview_section(&mut h, scan, "/p", &["/p/a.jpg".into(), "/p/b.mp4".into()]);
    overview_section(&mut h, scan, "/p/x/y", &["/p/x/y/c.jpg".into()]);
    h.get_by_label("x/y");
    h.get_by_role_and_label(Role::Label, "a.jpg");
    h.get_by_role_and_label(Role::Label, "b.mp4");
    h.get_by_role_and_label(Role::Label, "c.jpg");
    assert!(h.query_by_role(Role::Image).is_none());

    h.state_mut().core.handle(Event::ThumbnailDecoded {
        path: PathBuf::from("/p/x/y/c.jpg"),
        size: 160,
        result: Ok(DecodedImage {
            width: 2,
            height: 1,
            rgba: Arc::from(vec![255; 8]),
        }),
    });
    h.run();
    h.get_by_role_and_label(Role::Image, "c.jpg");

    h.key_press(egui::Key::G);
    h.run();
    assert!(h.query_by_label("x/y").is_none());
    h.get_by_role_and_label(Role::Image, "a.jpg");
}

#[test]
fn only_the_tiles_on_screen_are_laid_out_and_decoded() {
    let (mut h, scan) = overview_open();
    let scan = scan.unwrap();
    let files: Vec<String> = (0..500).map(|i| format!("/p/{i}.jpg")).collect();
    overview_section(&mut h, scan, "/p", &files);
    h.get_by_role_and_label(Role::Label, "0.jpg");
    assert!(h.query_by_role_and_label(Role::Label, "499.jpg").is_none());
    let visible = h
        .state()
        .core
        .view()
        .overview
        .as_ref()
        .unwrap()
        .visible
        .clone();
    assert_eq!(visible.start, 0);
    assert!(visible.end > 4 && visible.end < 100, "{visible:?}");
    let asked: Vec<&Command> = h
        .state()
        .commands
        .iter()
        .filter(|c| matches!(c, Command::DecodeThumbnail { .. }))
        .collect();
    assert_eq!(
        asked,
        (0..4)
            .map(|i| Command::DecodeThumbnail {
                path: PathBuf::from(format!("/p/{i}.jpg")),
                size: 160,
            })
            .collect::<Vec<_>>()
            .iter()
            .collect::<Vec<_>>()
    );
}

/// The overview's selected tile.
fn selected_tile(h: &Harness<'static, State>) -> Option<usize> {
    h.state().core.view().overview.as_ref()?.selected
}

#[test]
fn arrows_move_a_marked_selection_scrolled_into_view_only_once() {
    let (mut h, scan) = overview_open();
    let files: Vec<String> = (0..500).map(|i| format!("/p/{i}.jpg")).collect();
    overview_section(&mut h, scan.unwrap(), "/p", &files);
    assert_eq!(band_selected(&h, "0.jpg"), Some(true));
    for _ in 0..20 {
        h.key_press(egui::Key::ArrowDown);
    }
    h.run();
    let index = selected_tile(&h).unwrap();
    assert!(index >= 20, "{index}");
    let label = format!("{index}.jpg");
    assert_eq!(band_selected(&h, &label), Some(true));
    assert!(h.query_by_role_and_label(Role::Button, "0.jpg").is_none());

    // Scrolled away by the wheel, the view stays where the wheel left it.
    h.hover_at(egui::pos2(400.0, 300.0));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 100_000.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    // The wheel scrolls smoothly over many frames.
    h.run_steps(100);
    h.get_by_role_and_label(Role::Button, "0.jpg");
    assert!(h.query_by_role_and_label(Role::Button, &label).is_none());
}

#[test]
fn a_click_on_a_tile_shows_its_file() {
    let (mut h, scan) = overview_open();
    overview_section(
        &mut h,
        scan.unwrap(),
        "/p",
        &["/p/a.jpg".into(), "/p/b.jpg".into()],
    );
    h.get_by_role_and_label(Role::Button, "b.jpg").click();
    h.run();
    assert!(h.state().core.view().overview.is_none());
    assert!(matches!(
        &h.state().core.view().screen,
        Screen::Loading { path } if path == Path::new("/p/b.jpg")
    ));
}

#[test]
fn ctrl_wheel_over_the_overview_resizes_the_thumbnails() {
    let (mut h, scan) = overview_open();
    overview_section(&mut h, scan.unwrap(), "/p", &["/p/a.jpg".into()]);
    h.hover_at(egui::pos2(400.0, 300.0));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, 1.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::COMMAND,
    });
    h.run();
    assert_eq!(h.state().core.view().settings.thumbnail_size, 200);
}

#[test]
fn a_touchpad_steps_the_tiles_once_per_50_points_of_ctrl_scroll() {
    let (mut h, scan) = overview_open();
    overview_section(&mut h, scan.unwrap(), "/p", &["/p/a.jpg".into()]);
    h.hover_at(egui::pos2(400.0, 300.0));
    let mut scroll = |y: f32| {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, y),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::COMMAND,
        });
        h.run();
        h.state().core.view().settings.thumbnail_size
    };
    // 45 points: not a step yet.
    for _ in 0..9 {
        assert_eq!(scroll(5.0), 160);
    }
    assert_eq!(scroll(5.0), 200);
    for _ in 0..9 {
        assert_eq!(scroll(5.0), 200);
    }
    assert_eq!(scroll(5.0), 256);
    // Turning back starts counting afresh.
    assert_eq!(scroll(-30.0), 256);
    assert_eq!(scroll(-30.0), 200);
}

#[test]
fn video_and_live_photo_tiles_carry_their_marks_and_the_key_line_the_gallery_keys() {
    let (mut h, scan) = overview_open();
    overview_section(
        &mut h,
        scan.unwrap(),
        "/p",
        &[
            "/p/a.heic".into(),
            "/p/a.mov".into(),
            "/p/b.mp4".into(),
            "/p/c.jpg".into(),
        ],
    );
    h.get_by_role_and_label(Role::Button, "a.heic (live photo)");
    h.get_by_role_and_label(Role::Button, "b.mp4 (video)");
    let image = DecodedImage {
        width: 2,
        height: 1,
        rgba: Arc::from(vec![255; 8]),
    };
    for (path, result) in [
        ("/p/b.mp4", Ok(image)),
        ("/p/c.jpg", Err("broken".to_owned())),
    ] {
        h.state_mut().core.handle(Event::ThumbnailDecoded {
            path: PathBuf::from(path),
            size: 160,
            result,
        });
    }
    h.run();
    h.get_by_role_and_label(Role::Image, "b.mp4");
    h.get_by_role_and_label(Role::Button, "b.mp4 (video)");
    h.get_by_role_and_label(Role::Button, "c.jpg (error)");
    h.get_by_label("thumbnail size");
    h.get_by_label("close overview");
}

/// Showing `/p/a.jpg`, 2000 × 1000 pixels, in a folder with `b.jpg`.
fn big_browsing() -> Core {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/a.jpg")));
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/a.jpg"),
        result: Ok(DecodedImage {
            width: 2000,
            height: 1000,
            rgba: Arc::from(vec![0; 2000 * 1000 * 4]),
        }),
    });
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![PathBuf::from("/p/a.jpg"), PathBuf::from("/p/b.jpg")]),
    });
    core
}

/// Drags over the image from `from` to `to`.
fn drag_over(h: &mut Harness<'static, State>, from: egui::Pos2, to: egui::Pos2) {
    h.hover_at(from);
    h.run();
    h.drag_at(from);
    h.run();
    h.hover_at(to);
    h.run();
    h.drop_at(to);
    h.run();
}

fn shape_count(h: &Harness<'static, State>) -> usize {
    let edits = h.state().core.view().edits.as_ref();
    edits.map_or(0, |e| e.shapes.len())
}

#[test]
fn p_shows_the_toolbar_and_its_controls_reach_the_core() {
    use pixpixpix::core::{Color, Size};
    let mut h = harness(big_browsing());
    h.run();
    assert!(h.query_by_label("Arrow").is_none());
    h.key_press(egui::Key::P);
    h.run();
    h.get_by_role_and_label(Role::Button, "Arrow");
    for label in [
        "red", "yellow", "green", "white", "black", "S", "M", "Undo", "Redo",
    ] {
        h.get_by_role_and_label(Role::Button, label);
    }
    h.get_by_role_and_label(Role::Button, "cyan").click();
    h.run();
    h.get_by_role_and_label(Role::Button, "L").click();
    h.run();
    let toolbar = h.state().core.view().toolbar.unwrap();
    assert_eq!((toolbar.color, toolbar.size), (Color::Cyan, Size::Large));
    h.get_by_role_and_label(Role::Button, "Settings").click();
    h.run();
    assert!(h.state().core.view().settings_open);
}

#[test]
fn a_drag_with_the_toolbar_draws_an_arrow_instead_of_panning() {
    let mut h = harness(big_browsing());
    h.run();
    h.key_press(egui::Key::P);
    h.run();
    let zoom = h.state().core.view().zoom;
    drag_over(&mut h, egui::pos2(300.0, 250.0), egui::pos2(500.0, 300.0));
    assert_eq!(shape_count(&h), 1);
    assert_eq!(h.state().core.view().zoom, zoom);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run();
    assert_eq!(shape_count(&h), 0);
    h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::Z,
    );
    h.run();
    assert_eq!(shape_count(&h), 1);
}

/// The toolbar shown, an arrow drawn and the save dialog opened with Enter.
fn save_dialog() -> Harness<'static, State> {
    let mut h = harness(big_browsing());
    h.run();
    h.key_press(egui::Key::P);
    h.run();
    drag_over(&mut h, egui::pos2(300.0, 250.0), egui::pos2(500.0, 300.0));
    h.key_press(egui::Key::Enter);
    h.run();
    h.state_mut().commands.clear();
    h
}

fn saves(h: &Harness<'static, State>) -> Vec<PathBuf> {
    h.state()
        .commands
        .iter()
        .filter_map(|c| match c {
            Command::SaveEdits { target, .. } => Some(target.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_save_dialog_offers_all_four_choices() {
    let mut h = save_dialog();
    for label in [
        "Overwrite",
        "Save as new",
        "Copy to clipboard",
        "Undo edits",
        "Back to editing",
    ] {
        h.get_by_role_and_label(Role::Button, label);
    }
    h.get_by_role_and_label(Role::TextInput, "New file");
    h.get_by_label_contains("Enter: save as new");
    h.get_by_role_and_label(Role::Button, "Undo edits").click();
    h.run();
    assert_eq!(h.state().core.view().save_dialog, None);
    assert_eq!(shape_count(&h), 0);
}

#[test]
fn the_save_dialog_of_a_video_frame_has_no_overwrite() {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/clip.mp4")));
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/clip.mp4"),
        result: Ok(DecodedImage {
            width: 2000,
            height: 1000,
            rgba: Arc::from(vec![0; 2000 * 1000 * 4]),
        }),
    });
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![PathBuf::from("/p/clip.mp4")]),
    });
    let mut h = harness(core);
    h.run();
    h.get_by_role_and_label(Role::Button, "Play");
    h.key_press(egui::Key::P);
    h.run();
    // The tools draw where the play triangle was.
    assert!(h.query_by_role_and_label(Role::Button, "Play").is_none());
    drag_over(&mut h, egui::pos2(300.0, 250.0), egui::pos2(500.0, 300.0));
    assert_eq!(shape_count(&h), 1);
    h.key_press(egui::Key::Enter);
    h.run();
    for label in [
        "Save as new",
        "Copy to clipboard",
        "Undo edits",
        "Back to editing",
    ] {
        h.get_by_role_and_label(Role::Button, label);
    }
    assert!(
        h.query_by_role_and_label(Role::Button, "Overwrite")
            .is_none()
    );
    let name = h.get_by_role_and_label(Role::TextInput, "New file").value();
    assert_eq!(name.as_deref(), Some("clip_00-00-00.000.png"));
    assert!(
        !h.state()
            .commands
            .iter()
            .any(|c| matches!(c, Command::PlayVideo { .. }))
    );
}

#[test]
fn enter_in_the_save_dialog_saves_as_new_once() {
    let mut h = save_dialog();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(saves(&h), [PathBuf::from("/p/a_edit.jpg")]);
}

#[test]
fn a_typed_name_is_saved_once_with_enter() {
    let mut h = save_dialog();
    h.get_by_role_and_label(Role::TextInput, "New file").focus();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "New file")
        .type_text("x");
    h.run();
    let name = h
        .state()
        .core
        .view()
        .save_dialog
        .as_ref()
        .unwrap()
        .name
        .clone();
    assert!(name.contains('x') && name != "a_edit.jpg", "{name}");
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(saves(&h), [PathBuf::from("/p").join(name)]);
}

#[test]
fn esc_in_the_save_dialog_goes_back_to_editing() {
    let mut h = save_dialog();
    h.key_press(egui::Key::Escape);
    h.run();
    assert_eq!(h.state().core.view().save_dialog, None);
    assert_eq!(shape_count(&h), 1);
    assert!(saves(&h).is_empty());
}

#[test]
fn leaving_the_image_with_edits_shows_the_save_dialog() {
    let mut h = harness(big_browsing());
    h.run();
    h.key_press(egui::Key::P);
    h.run();
    drag_over(&mut h, egui::pos2(300.0, 250.0), egui::pos2(500.0, 300.0));
    h.key_press(egui::Key::ArrowRight);
    h.run();
    h.get_by_role_and_label(Role::Button, "Save as new");
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Q);
    h.run();
    assert!(!h.state().commands.contains(&Command::Quit));
}

fn choose_tool(h: &mut Harness<'static, State>, name: &str) {
    h.get_by_role_and_label(Role::Button, name).click();
    h.run();
}

#[test]
fn every_drawing_tool_has_its_button() {
    use pixpixpix::core::Tool;
    let mut h = harness(big_browsing());
    h.run();
    h.key_press(egui::Key::P);
    h.run();
    for tool in Tool::ALL {
        choose_tool(&mut h, tool.name());
        assert_eq!(h.state().core.view().toolbar.unwrap().tool, tool);
    }
}

#[test]
fn text_is_typed_where_clicked_and_enter_ends_it_without_saving() {
    use pixpixpix::core::Shape;
    let mut h = harness(big_browsing());
    h.run();
    h.key_press(egui::Key::P);
    h.run();
    choose_tool(&mut h, "Text");
    h.hover_at(egui::pos2(300.0, 250.0));
    h.run();
    // Pressed and released in place: a click.
    h.drag_at(egui::pos2(300.0, 250.0));
    h.run();
    h.drop_at(egui::pos2(300.0, 250.0));
    h.run();
    h.run();
    h.get_by_role(Role::TextInput).type_text("hi");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    h.run();
    let edits = h.state().core.view().edits.clone().unwrap();
    assert!(
        matches!(&edits.shapes[..], [Shape::Text { text, .. }] if text == "hi"),
        "{edits:?}"
    );
    assert_eq!(edits.drawing, None);
    assert_eq!(h.state().core.view().save_dialog, None);
    assert!(h.query_by_role(Role::TextInput).is_none());
}

#[test]
fn a_crop_shows_only_the_part_kept_once_another_tool_is_chosen() {
    let mut h = harness(big_browsing());
    h.run();
    h.key_press(egui::Key::P);
    h.run();
    choose_tool(&mut h, "Crop");
    drag_over(&mut h, egui::pos2(300.0, 250.0), egui::pos2(500.0, 300.0));
    assert_eq!(shape_count(&h), 1);
    assert_eq!(h.state().core.view().crop, None);
    choose_tool(&mut h, "Pen");
    assert!(h.state().core.view().crop.is_some());
    h.get_by_role_and_label(Role::Image, "a.jpg");
}

#[test]
fn box_tools_and_the_pen_draw_with_a_drag() {
    use pixpixpix::core::Shape;
    let mut h = harness(big_browsing());
    h.run();
    h.key_press(egui::Key::P);
    h.run();
    for name in ["Rectangle", "Ellipse", "Pen", "Highlighter", "Pixelate"] {
        choose_tool(&mut h, name);
        drag_over(&mut h, egui::pos2(300.0, 250.0), egui::pos2(500.0, 300.0));
        h.run();
    }
    let shapes = h.state().core.view().edits.clone().unwrap().shapes;
    assert!(
        matches!(
            &shapes[..],
            [
                Shape::Rectangle { .. },
                Shape::Ellipse { .. },
                Shape::Pen { .. },
                Shape::Highlighter { .. },
                Shape::Pixelate { .. },
            ]
        ),
        "{shapes:?}"
    );
}

/// Sets the number field `label` as a screen reader would.
fn set_number(h: &mut Harness<'static, State>, label: &str, value: f64) {
    use egui::accesskit::{Action, ActionData, ActionRequest};
    let node = h.get_by_role_and_label(Role::SpinButton, label);
    let (target_node, target_tree) = node.accesskit_node().locate();
    h.event(egui::Event::AccessKitActionRequest(ActionRequest {
        action: Action::SetValue,
        target_node,
        target_tree,
        data: Some(ActionData::NumericValue(value)),
    }));
    h.run();
}

/// The toolbar shown with the export tool chosen.
fn export_tool() -> Harness<'static, State> {
    let mut h = harness(big_browsing());
    h.run();
    h.key_press(egui::Key::P);
    h.run();
    choose_tool(&mut h, "Export");
    h
}

#[test]
fn the_export_tool_shows_its_choices_instead_of_the_palette() {
    use pixpixpix::core::ExportFormat;
    let mut h = export_tool();
    assert!(h.query_by_role_and_label(Role::Button, "red").is_none());
    for label in ["Width", "Height", "Max KB"] {
        h.get_by_role_and_label(Role::SpinButton, label);
    }
    set_number(&mut h, "Width", 512.0);
    set_number(&mut h, "Height", 256.0);
    set_number(&mut h, "Max KB", 300.0);
    let lock = |h: &Harness<'static, State>| {
        h.get_by_role_and_label(Role::Button, "Lock ratio")
            .accesskit_node()
            .toggled()
    };
    assert_eq!(lock(&h), Some(Toggled::True));
    h.get_by_role_and_label(Role::Button, "Lock ratio").click();
    h.run();
    assert_eq!(lock(&h), Some(Toggled::False));
    h.get_by_role_and_label(Role::Button, "WebP").click();
    h.run();
    assert!(
        h.query_by_role_and_label(Role::SpinButton, "Max KB")
            .is_none()
    );
    let export = h.state().core.view().export.unwrap().export;
    assert_eq!(
        (export.width, export.height, export.limit_kb),
        (512, 256, Some(300))
    );
    assert!(!export.locked);
    assert_eq!(export.format, ExportFormat::WebP);
}

#[test]
fn a_drag_with_the_export_tool_draws_the_area() {
    let mut h = export_tool();
    let before = h.state().core.view().export.unwrap().area;
    drag_over(&mut h, egui::pos2(300.0, 250.0), egui::pos2(500.0, 300.0));
    let after = h.state().core.view().export.unwrap().area;
    assert!(after.is_some() && after != before, "{after:?}");
    assert_eq!(shape_count(&h), 0);
}

#[test]
fn export_opens_the_save_dialog_with_the_size_in_the_name() {
    let mut h = export_tool();
    set_number(&mut h, "Width", 512.0);
    set_number(&mut h, "Height", 512.0);
    h.get_by_role_and_label(Role::Button, "PNG").click();
    h.run();
    h.get_by_role_and_label(Role::Button, "Export\u{2026}")
        .click();
    h.run();
    let name = h.get_by_role_and_label(Role::TextInput, "New file").value();
    assert_eq!(name.as_deref(), Some("a_512x512.png"));
    for label in ["Save as new", "Copy to clipboard", "Back to editing"] {
        h.get_by_role_and_label(Role::Button, label);
    }
    assert!(
        h.query_by_role_and_label(Role::Button, "Undo edits")
            .is_none()
    );
    assert!(
        h.get_by_role_and_label(Role::Button, "Overwrite")
            .accesskit_node()
            .is_disabled()
    );
    h.get_by_label_contains("an export never replaces the original");
    h.state_mut().commands.clear();
    h.key_press(egui::Key::Enter);
    h.run();
    let targets: Vec<PathBuf> = h
        .state()
        .commands
        .iter()
        .filter_map(|c| match c {
            Command::Export { target, .. } => Some(target.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(targets, [PathBuf::from("/p/a_512x512.png")]);
}

/// The metadata view open on a.jpg, its capture date read.
fn metadata_open() -> Harness<'static, State> {
    use pixpixpix::metadata::{Field, Metadata};
    let mut h = harness(browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]));
    h.run();
    h.key_press(egui::Key::I);
    h.run();
    assert!(
        h.state()
            .commands
            .contains(&Command::ReadMetadata(PathBuf::from("/p/a.jpg")))
    );
    h.state_mut().core.handle(Event::MetadataRead {
        path: PathBuf::from("/p/a.jpg"),
        result: Ok(Metadata {
            file: vec![Field {
                label: "File",
                value: "a.jpg".into(),
            }],
            captured: Some("2019-06-07 08:09:10".into()),
            exif: vec![Field {
                label: "Camera",
                value: "Pixcam".into(),
            }],
            has_exif: true,
            has_location: true,
        }),
    });
    h.run();
    h.state_mut().commands.clear();
    h
}

#[test]
fn i_shows_the_metadata_with_the_capture_date_in_a_field() {
    let mut h = metadata_open();
    h.get_by_label("Metadata");
    h.get_by_label("Pixcam");
    h.get_by_label("2 × 1");
    let date = h.get_by_role_and_label(Role::TextInput, "Captured").value();
    assert_eq!(date.as_deref(), Some("2019-06-07 08:09:10"));
    h.key_press(egui::Key::I);
    h.run();
    assert!(h.query_by_label("Metadata").is_none());
}

#[test]
fn keys_typed_into_the_date_field_stay_there_and_enter_checks_the_date() {
    let mut h = metadata_open();
    h.get_by_role_and_label(Role::TextInput, "Captured").focus();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Captured")
        .type_text("x");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    // Neither trashed nor written: the date is no date.
    assert_eq!(h.state().commands, []);
    h.get_by_label("Enter the date as YYYY-MM-DD HH:MM:SS");
}

#[test]
fn enter_in_the_date_field_writes_a_valid_date() {
    let mut h = metadata_open();
    h.state_mut()
        .core
        .handle(Event::DateEdited("2021-12-24 18:30:00".into()));
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Captured").focus();
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(
        h.state().commands,
        [Command::WriteDate {
            path: PathBuf::from("/p/a.jpg"),
            date: "2021:12:24 18:30:00".into()
        }]
    );
}

#[test]
fn remove_location_in_the_metadata_view_goes_through_the_save_dialog() {
    use pixpixpix::metadata::Strip;
    let mut h = metadata_open();
    h.get_by_role_and_label(Role::Button, "Remove all EXIF");
    h.get_by_role_and_label(Role::Button, "Remove location")
        .click();
    h.run();
    h.get_by_label_contains("Remove the location from");
    for label in ["Overwrite", "Save as new", "Copy to clipboard", "Back"] {
        h.get_by_role_and_label(Role::Button, label);
    }
    assert!(h.query_by_label("Undo edits").is_none());
    let name = h.get_by_role_and_label(Role::TextInput, "New file").value();
    assert_eq!(name.as_deref(), Some("a_noloc.jpg"));
    h.get_by_role_and_label(Role::Button, "Save as new").click();
    h.run();
    assert_eq!(
        h.state().commands,
        [Command::RemoveMetadata {
            source: PathBuf::from("/p/a.jpg"),
            strip: Strip::Location,
            target: PathBuf::from("/p/a_noloc.jpg"),
        }]
    );
}

#[test]
fn files_still_loading_stand_in_the_band_as_placeholders() {
    let core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg", "/p/c.jpg"]);
    let mut h = harness(core);
    h.run();
    h.get_by_role_and_label(Role::Button, "b.jpg (loading)");
    h.get_by_role_and_label(Role::Button, "c.jpg (loading)");
}

#[test]
fn the_current_thumb_is_in_the_middle_of_the_band() {
    let files = ["/p/a.jpg", "/p/b.jpg", "/p/c.jpg", "/p/d.jpg", "/p/e.jpg"];
    let mut core = browsing("/p/a.jpg", &files);
    for name in &files[1..] {
        decoded(&mut core, name);
    }
    core.handle(Event::Show(PathBuf::from("/p/c.jpg")));
    let mut h = harness(core);
    h.run();
    let middle = h.ctx.content_rect().center().x;
    let x = |name: &str| {
        h.get_by_role_and_label(Role::Button, name)
            .rect()
            .center()
            .x
    };
    assert!(
        (x("c.jpg") - middle).abs() < 1.0,
        "{} vs {middle}",
        x("c.jpg")
    );
    assert!(x("b.jpg") < x("c.jpg") && x("c.jpg") < x("d.jpg"));
}

#[test]
fn at_the_first_file_the_left_of_the_band_stays_empty() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]);
    decoded(&mut core, "/p/b.jpg");
    let mut h = harness(core);
    h.run();
    let middle = h.ctx.content_rect().center().x;
    let a = h.get_by_role_and_label(Role::Button, "a.jpg").rect();
    let b = h.get_by_role_and_label(Role::Button, "b.jpg").rect();
    assert!((a.center().x - middle).abs() < 1.0);
    assert!(b.min.x > a.max.x);
}

#[test]
fn the_band_key_is_named_preloaded_files() {
    let mut h = harness(browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]));
    h.run();
    // B and Tab both lead there.
    assert_eq!(h.query_all_by_label("preloaded files").count(), 2);
}

#[test]
fn a_click_on_a_folder_goes_there_and_dot_dot_goes_up() {
    let mut h = column_open();
    h.get_by_role_and_label(Role::Button, "q").click();
    h.run();
    assert_eq!(
        h.state().core.view().screen,
        Screen::Listing {
            dir: PathBuf::from("/q")
        }
    );
    h.get_by_role_and_label(Role::Button, "..").click();
    h.run();
    assert_eq!(
        h.state().core.view().screen,
        Screen::Listing {
            dir: PathBuf::from("/")
        }
    );
    // The root stands alone, without `..`.
    assert!(h.query_by_role_and_label(Role::Button, "..").is_none());
}

#[test]
fn the_key_line_wraps_in_a_narrow_window() {
    let mut h = harness(browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]));
    h.set_size(egui::vec2(320.0, 600.0));
    h.run();
    let width = h.ctx.content_rect().width();
    for keys in ["Left/Right", "+/-", "G", "N", "S", "F"] {
        let rect = h.get_by_label(keys).rect();
        assert!(
            rect.max.x <= width,
            "{keys} ends at {} of {width}",
            rect.max.x
        );
    }
    let quit = h
        .get_by_label(if cfg!(target_os = "macos") {
            "Cmd+Q"
        } else {
            "Ctrl+Q"
        })
        .rect();
    assert!(quit.max.x <= width);
    // It took more than one line.
    assert!(quit.min.y > h.get_by_label("Left/Right").rect().max.y);
}

#[test]
fn question_mark_hides_the_hints_but_not_the_tags_and_is_saved() {
    let mut h = column_open();
    h.key_press(egui::Key::Period);
    h.run();
    h.state_mut().commands.clear();
    h.key_press(egui::Key::Questionmark);
    h.run();
    assert!(h.query_by_label("Up/Down").is_none());
    h.get_by_label("?");
    h.get_by_label("dot entries shown");
    assert_eq!(
        h.state().commands,
        [Command::SaveSettings(Settings {
            hints_hidden: true,
            ..Settings::default()
        })]
    );
    h.key_press(egui::Key::Questionmark);
    h.run();
    h.get_by_label("Up/Down");
    assert!(h.query_by_label("?").is_none());
}

#[test]
fn hidden_hints_stay_hidden_after_a_restart() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg"]);
    core.handle(Event::SettingsLoaded(Ok(Settings {
        hints_hidden: true,
        ..Settings::default()
    })));
    let mut h = harness(core);
    h.run();
    assert!(h.query_by_label("Left/Right").is_none());
    h.get_by_label("?");
}

/// A click of the primary button at `pos`.
fn click_at(h: &mut Harness<'static, State>, pos: egui::Pos2) {
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run();
}

#[test]
fn a_click_on_a_key_hint_presses_the_key_under_the_pointer() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]);
    decoded(&mut core, "/p/b.jpg");
    let mut h = harness(core);
    h.run();
    // The right end of `Left/Right` is `Right`.
    let rect = h.get_by_label("Left/Right").rect();
    click_at(&mut h, egui::pos2(rect.max.x - 3.0, rect.center().y));
    assert!(matches!(
        &h.state().core.view().screen,
        Screen::Image { path, .. } if path == Path::new("/p/b.jpg")
    ));
    // A one-key hint's action presses it too.
    let action = h
        .query_all_by_label("preloaded files")
        .next()
        .unwrap()
        .rect();
    click_at(&mut h, action.center());
    assert_eq!(h.state().core.view().focus, pixpixpix::core::Focus::Band);
}

#[test]
fn an_arrow_shows_near_the_edge_and_steps_to_the_next_file() {
    let mut h = harness(browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]));
    h.run();
    assert!(h.query_by_label("Next").is_none());
    let area = h.ctx.content_rect();
    h.hover_at(egui::pos2(area.max.x - 20.0, area.center().y));
    h.run();
    assert!(h.query_by_label("Previous").is_none());
    h.get_by_role_and_label(Role::Button, "Next").click();
    h.run();
    assert_eq!(
        h.state().core.view().screen,
        Screen::Loading {
            path: PathBuf::from("/p/b.jpg")
        }
    );
}

#[test]
fn the_toolbar_saves_and_closes_by_click() {
    let mut h = harness(shown("/p/cat.jpg"));
    h.run();
    h.key_press(egui::Key::P);
    h.run();
    assert!(
        h.get_by_role_and_label(Role::Button, "Save\u{2026}")
            .accesskit_node()
            .is_disabled()
    );
    h.get_by_role_and_label(Role::Button, "Close").click();
    h.run();
    assert_eq!(h.state().core.view().toolbar, None);
}

/// Whether the diode of `area` is lit.
fn diode_lit(h: &Harness<'_, State>, area: &str) -> Option<bool> {
    h.get_by_label(&format!("{area} focus"))
        .accesskit_node()
        .is_selected()
}

#[test]
fn the_key_line_names_the_focused_area_and_its_diode_is_lit() {
    let mut h = column_open();
    assert_eq!(diode_lit(&h, "folders"), Some(true));
    assert_eq!(diode_lit(&h, "image view"), Some(false));
    assert_eq!(diode_lit(&h, "preloaded files"), Some(false));
    h.key_press(egui::Key::Tab);
    h.run();
    assert_eq!(diode_lit(&h, "folders"), Some(false));
    assert_eq!(diode_lit(&h, "image view"), Some(true));
    // The key line starts with the area's name.
    let first = h.get_by_label("image view").rect();
    assert!(first.min.x < h.get_by_label("Left/Right").rect().min.x);
    h.key_press(egui::Key::Tab);
    h.run();
    assert_eq!(diode_lit(&h, "preloaded files"), Some(true));
    assert_eq!(diode_lit(&h, "image view"), Some(false));
}

#[test]
fn an_error_has_a_copy_button_that_reports_it_with_its_file() {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/bad.jpg")));
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/bad.jpg"),
        result: Err("unexpected end of file".into()),
    });
    let mut h = harness(core);
    h.run();
    h.get_by_role_and_label(Role::Button, "Copy").click();
    h.run();
    assert_eq!(
        h.state().commands,
        [Command::CopyReport {
            message: "unexpected end of file".into(),
            path: Some(PathBuf::from("/p/bad.jpg")),
        }]
    );
}

#[test]
fn the_settings_screen_lines_its_controls_up_in_sections() {
    let mut h = harness(shown("/p/cat.jpg"));
    h.run();
    h.key_press(egui::Key::S);
    h.run();
    h.get_by_label("Browsing");
    h.get_by_label("Other apps");
    let slider = h.get_by_role_and_label(Role::Slider, CACHE_LABEL).rect();
    let image_app = h
        .get_by_role_and_label(Role::TextInput, "App for images")
        .rect();
    let video_app = h
        .get_by_role_and_label(Role::TextInput, "App for videos")
        .rect();
    // The controls start in one column, right of every label.
    assert!((slider.min.x - image_app.min.x).abs() < 1.0);
    assert!((image_app.min.x - video_app.min.x).abs() < 1.0);
    // The labels' column comes first.
    let left = h.get_by_label("Browsing").rect().min.x;
    assert!(slider.min.x >= left + 170.0, "{left} {slider:?}");
    // Rows do not overlap, and everything is in the window.
    assert!(image_app.min.y >= slider.max.y);
    assert!(video_app.min.y >= image_app.max.y);
    let window = h.ctx.content_rect();
    for rect in [slider, image_app, video_app] {
        assert!(window.contains_rect(rect), "{rect:?}");
    }
}

#[test]
fn the_settings_link_to_an_about_page_that_lists_and_copies_the_libraries() {
    let mut h = harness(shown("/p/cat.jpg"));
    h.run();
    h.key_press(egui::Key::S);
    h.run();
    h.state_mut().commands.clear();
    let about = h.get_by_role_and_label(Role::Button, "About pixpixpix");
    assert!(
        h.ctx.content_rect().contains_rect(about.rect()),
        "{:?}",
        about.rect()
    );
    about.click();
    h.run();
    h.run();
    assert_eq!(h.state().commands, [Command::ProbeLibraries]);
    h.get_by_label("pixpixpix");
    h.get_by_label(&format!(
        "Version {} \u{b7} MIT license",
        env!("CARGO_PKG_VERSION")
    ));
    h.get_by_label("Looking\u{2026}");
    h.state_mut().core.handle(Event::LibrariesProbed(vec![
        pixpixpix::core::LibraryStatus {
            name: "libmpv".into(),
            needed_for: "videos".into(),
            found: None,
            hint: "mpv-libs on Fedora".into(),
        },
    ]));
    h.run();
    h.get_by_label("missing");
    h.get_by_label("mpv-libs on Fedora");
    h.state_mut().commands.clear();
    h.get_by_role_and_label(Role::Button, "Copy").click();
    h.run();
    let [Command::CopyText(text)] = h.state().commands.as_slice() else {
        panic!("{:?}", h.state().commands);
    };
    assert!(
        text.contains("libmpv (videos): missing (mpv-libs on Fedora)"),
        "{text}"
    );
    assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");
    // Esc goes back to the settings, S closes them.
    h.key_press(egui::Key::Escape);
    h.run();
    h.get_by_label("Browsing");
    h.key_press(egui::Key::S);
    h.run();
    assert!(h.query_by_label("Browsing").is_none());
}

#[test]
fn the_settings_choose_pixpixpix_as_the_default_app_of_a_type_or_of_all() {
    let mut h = harness(shown("/p/cat.jpg"));
    h.run();
    h.key_press(egui::Key::S);
    h.run();
    h.get_by_label("Default apps");
    let ours = "io.github.paxel.pixpixpix.desktop";
    h.state_mut()
        .core
        .handle(Event::DefaultsRead(pixpixpix::core::Defaults {
            rows: vec![
                pixpixpix::core::DefaultRow {
                    name: "JPEG".into(),
                    video: false,
                    current: Some("gwenview.desktop".into()),
                    ours: false,
                },
                pixpixpix::core::DefaultRow {
                    name: "PNG".into(),
                    video: false,
                    current: None,
                    ours: false,
                },
            ],
            names: [("gwenview.desktop".to_owned(), "Gwenview".to_owned())].into(),
            ours: ours.into(),
            read_only: false,
            error: None,
        }));
    h.run();
    let jpeg = h.get_by_role_and_label(Role::ComboBox, "JPEG");
    assert_eq!(jpeg.accesskit_node().value().as_deref(), Some("Gwenview"));
    h.state_mut().commands.clear();
    h.get_by_role_and_label(Role::CheckBox, "pixpixpix for all images")
        .click();
    h.run();
    assert!(h.state().commands.contains(&Command::SetDefaults {
        names: vec!["JPEG".into(), "PNG".into()],
        app: ours.into(),
    }));
}

#[test]
fn a_hidden_band_gives_its_space_to_the_image() {
    let mut core = browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]);
    decoded(&mut core, "/p/b.jpg");
    let mut h = harness(core);
    h.run();
    let before = h.get_by_role_and_label(Role::Image, "a.jpg").rect();
    h.get_by_role_and_label(Role::Button, "b.jpg");
    h.key_press(egui::Key::B);
    h.run();
    h.key_press(egui::Key::B);
    h.run();
    assert!(h.query_by_role_and_label(Role::Button, "b.jpg").is_none());
    let after = h.get_by_role_and_label(Role::Image, "a.jpg").rect();
    assert!(after.height() >= before.height());
}

#[test]
fn the_settings_screen_fits_the_window_with_every_default_app_listed() {
    let mut h = harness(shown("/p/cat.jpg"));
    h.run();
    h.key_press(egui::Key::S);
    h.run();
    let rows = pixpixpix::associations::TYPES
        .iter()
        .map(|t| pixpixpix::core::DefaultRow {
            name: t.name.into(),
            video: t.video,
            current: None,
            ours: false,
        })
        .collect();
    h.state_mut()
        .core
        .handle(Event::DefaultsRead(pixpixpix::core::Defaults {
            rows,
            names: Default::default(),
            ours: "io.github.paxel.pixpixpix.desktop".into(),
            read_only: false,
            error: None,
        }));
    h.run();
    // Reachable: pointer clicks on the last switch and the About link work.
    h.state_mut().commands.clear();
    h.get_by_role_and_label(Role::CheckBox, "pixpixpix for all videos")
        .click();
    h.run();
    assert!(
        h.state()
            .commands
            .iter()
            .any(|c| matches!(c, Command::SetDefaults { .. })),
        "{:?}",
        h.state().commands
    );
    h.get_by_role_and_label(Role::Button, "About pixpixpix")
        .click();
    h.run();
    assert!(h.state().commands.contains(&Command::ProbeLibraries));
}
