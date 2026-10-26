//! Seam 2: the egui layer over the core, headless.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pixpixpix::core::{Command, Core, Event, Screen};
use pixpixpix::decode::DecodedImage;
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
    let (core, _) = Core::start(Err("No images in /x".into()));
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
