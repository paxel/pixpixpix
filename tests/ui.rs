//! Seam 2: the egui layer over the core, headless.

use std::path::PathBuf;
use std::sync::Arc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pixpixpix::core::{Command, Core, Event};
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
                ui::show(ui, state.core.view(), &mut state.textures);
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
    h.get_by_label("cat.jpg");
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
