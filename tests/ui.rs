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

/// [`browsing`] with the column opened by `n` and listing `holidays` and
/// `work`.
fn column_open() -> Harness<'static, State> {
    let mut h = harness(browsing("/p/a.jpg", &["/p/a.jpg", "/p/b.jpg"]));
    h.run();
    h.key_press(egui::Key::N);
    h.run();
    assert_eq!(
        h.state().commands,
        [
            Command::ListDirs(PathBuf::from("/p")),
            Command::ListFolder(PathBuf::from("/"))
        ]
    );
    h.state_mut().core.handle(Event::DirsListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![PathBuf::from("/p/holidays"), PathBuf::from("/p/work")]),
    });
    h.run();
    h
}

#[test]
fn n_shows_the_folder_and_its_subfolders_with_the_first_highlighted() {
    let mut h = column_open();
    h.get_by_label("p");
    let toggled = |h: &Harness<'_, State>, name: &str| {
        h.get_by_role_and_label(Role::Button, name)
            .accesskit_node()
            .toggled()
    };
    assert_eq!(toggled(&h, "holidays"), Some(Toggled::True));
    assert_eq!(toggled(&h, "work"), Some(Toggled::False));
    h.key_press(egui::Key::ArrowDown);
    h.run();
    assert_eq!(toggled(&h, "work"), Some(Toggled::True));
    h.key_press(egui::Key::N);
    h.run();
    assert!(h.query_by_label("holidays").is_none());
}

#[test]
fn enter_opens_the_highlighted_folder() {
    let mut h = column_open();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(
        h.state().core.view().screen,
        Screen::Listing {
            dir: PathBuf::from("/p/holidays")
        }
    );
    h.get_by_label("holidays");
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
    h.get_by_label("/");
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
fn the_highlighted_folder_is_previewed_with_the_band_following() {
    let mut h = column_open();
    listed(
        &mut h,
        "/p/holidays",
        Ok(vec![PathBuf::from("/p/holidays/beach.jpg")]),
    );
    h.state_mut().core.handle(Event::Decoded {
        path: PathBuf::from("/p/holidays/beach.jpg"),
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
fn a_folder_without_images_shows_a_placeholder_with_its_subfolder_count() {
    let mut h = column_open();
    h.key_press(egui::Key::ArrowDown);
    h.run();
    listed(&mut h, "/p/work", Ok(Vec::new()));
    h.get_by_label("No images");
    h.state_mut().core.handle(Event::DirsListed {
        dir: PathBuf::from("/p/work"),
        result: Ok(vec![
            PathBuf::from("/p/work/a"),
            PathBuf::from("/p/work/b"),
            PathBuf::from("/p/work/c"),
        ]),
    });
    h.run();
    h.get_by_label("No images · 3 folders");
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
fn r_in_the_column_searches_and_shows_progress_then_the_result() {
    let mut h = column_open();
    h.state_mut().commands.clear();
    h.key_press(egui::Key::R);
    h.run();
    assert_eq!(
        h.state().commands,
        [Command::Search {
            dir: PathBuf::from("/p"),
            dots: false
        }]
    );
    h.get_by_label("Searching…");
    h.state_mut().core.handle(Event::Searched {
        dir: PathBuf::from("/p"),
        dots: false,
        found: None,
    });
    h.run();
    h.get_by_label("No images below");
}

#[test]
fn dot_in_the_column_shows_dot_folders() {
    let mut h = column_open();
    h.state_mut().core.handle(Event::DirsListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![PathBuf::from("/p/.git"), PathBuf::from("/p/work")]),
    });
    h.run();
    assert!(h.query_by_label(".git").is_none());
    h.key_press(egui::Key::Period);
    h.run();
    h.get_by_label(".git");
}

/// Whether the band thumb `name` is highlighted for keyboard selection.
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
            app: None
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
    h.get_by_label("C");
    h.get_by_label("copy");
    h.key_press(egui::Key::R);
    h.key_press(egui::Key::C);
    h.run();
    assert!(matches!(
        h.state().commands.last(),
        Some(Command::Copy {
            path,
            rotation: pixpixpix::rotate::Rotation::Right,
            ..
        }) if path == Path::new("/p/a.jpg")
    ));
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
