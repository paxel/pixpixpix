//! Seam 1: the app core's `l`/`r` rotation on screen and `Enter` writing it.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Command, Core, Event, Key, KeyPress, Placement, Points, Scale, Screen};
use pixpixpix::decode::DecodedImage;
use pixpixpix::rotate::Rotation;

/// 400 × 200 in a 200 × 200 viewport.
fn wide() -> DecodedImage {
    DecodedImage {
        width: 400,
        height: 200,
        rgba: Arc::from(vec![0; 4]),
    }
}

/// `wide` turned: 200 × 400.
fn tall() -> DecodedImage {
    DecodedImage {
        width: 200,
        height: 400,
        rgba: Arc::from(vec![1; 4]),
    }
}

fn p(name: &str) -> PathBuf {
    PathBuf::from(format!("/p/{name}"))
}

fn press(core: &mut Core, key: Key) -> Vec<Command> {
    core.handle(Event::Key(KeyPress::plain(key)))
}

fn decoded(core: &mut Core, name: &str, image: DecodedImage) -> Vec<Command> {
    core.handle(Event::Decoded {
        path: p(name),
        result: Ok(image),
    })
}

/// Showing `a.jpg` of a folder with `a.jpg` and `b.jpg`, both decoded.
fn showing() -> Core {
    let (mut core, _) = Core::start(Ok(p("a.jpg")));
    decoded(&mut core, "a.jpg", wide());
    core.handle(Event::Viewport(Points { x: 200.0, y: 200.0 }));
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("a.jpg"), p("b.jpg")]),
    });
    decoded(&mut core, "b.jpg", wide());
    core
}

fn write(name: &str, rotation: Rotation) -> Command {
    Command::WriteRotation {
        path: p(name),
        rotation,
    }
}

fn shown_path(core: &Core) -> Option<PathBuf> {
    match &core.view().screen {
        Screen::Image { path, .. } => Some(path.clone()),
        _ => None,
    }
}

#[test]
fn r_and_l_turn_the_shown_image_in_quarter_steps() {
    let mut core = showing();
    assert_eq!(core.view().rotation, Rotation::None);
    for expected in [
        Rotation::Right,
        Rotation::Half,
        Rotation::Left,
        Rotation::None,
    ] {
        assert!(press(&mut core, Key::R).is_empty());
        assert_eq!(core.view().rotation, expected);
    }
    for expected in [Rotation::Left, Rotation::Half] {
        assert!(press(&mut core, Key::L).is_empty());
        assert_eq!(core.view().rotation, expected);
    }
}

#[test]
fn a_turned_image_is_placed_with_width_and_height_swapped() {
    let mut core = showing();
    press(&mut core, Key::R);
    let view = core.view();
    let Screen::Image { image, .. } = &view.screen else {
        panic!("no image");
    };
    assert_eq!(
        view.zoom.placement(image, view.rotation, view.viewport),
        Placement {
            left: 50.0,
            top: 0.0,
            width: 100.0,
            height: 200.0
        }
    );
}

#[test]
fn turning_keeps_the_image_point_in_the_middle_where_it_is() {
    let mut core = showing();
    press(&mut core, Key::Num1);
    core.handle(Event::Pan(Points { x: 40.0, y: 0.0 }));
    let before = core.view().zoom;
    assert!((before.center_x - 0.4).abs() < 1e-4);
    press(&mut core, Key::R);
    let zoom = core.view().zoom;
    assert_eq!(zoom.scale, Scale::Exact(1.0));
    // The point at 40 % of the width now sits at 40 % of the height.
    assert!((zoom.center_y - 0.4).abs() < 1e-4, "{zoom:?}");
    assert!((zoom.center_x - 0.5).abs() < 1e-4, "{zoom:?}");
    press(&mut core, Key::L);
    assert!((core.view().zoom.center_x - 0.4).abs() < 1e-4);
}

#[test]
fn zoom_works_on_the_turned_size() {
    let mut core = showing();
    press(&mut core, Key::R);
    // Turned, 200 × 400 fits at 50 %; a zoom step starts from there.
    press(&mut core, Key::Plus);
    assert_eq!(core.view().zoom.scale, Scale::Exact(0.625));
}

#[test]
fn nothing_turns_without_an_image() {
    let (mut core, _) = Core::start(Ok(p("a.jpg")));
    press(&mut core, Key::R);
    assert_eq!(core.view().rotation, Rotation::None);
    core.handle(Event::Decoded {
        path: p("a.jpg"),
        result: Err("broken".into()),
    });
    press(&mut core, Key::L);
    assert_eq!(core.view().rotation, Rotation::None);
    assert!(press(&mut core, Key::Enter).is_empty());
}

#[test]
fn leaving_the_image_drops_the_rotation_without_writing() {
    let mut core = showing();
    press(&mut core, Key::R);
    let commands = press(&mut core, Key::Right);
    assert!(
        !commands
            .iter()
            .any(|c| matches!(c, Command::WriteRotation { .. })),
        "{commands:?}"
    );
    assert_eq!(shown_path(&core), Some(p("b.jpg")));
    assert_eq!(core.view().rotation, Rotation::None);
    press(&mut core, Key::Left);
    assert_eq!(shown_path(&core), Some(p("a.jpg")));
    assert_eq!(core.view().rotation, Rotation::None);
}

#[test]
fn a_removed_current_file_drops_the_rotation() {
    let mut core = showing();
    press(&mut core, Key::R);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("b.jpg")]),
    });
    assert_eq!(shown_path(&core), Some(p("b.jpg")));
    assert_eq!(core.view().rotation, Rotation::None);
}

#[test]
fn enter_writes_the_pending_rotation() {
    let mut core = showing();
    assert!(press(&mut core, Key::Enter).is_empty());
    press(&mut core, Key::L);
    assert_eq!(
        press(&mut core, Key::Enter),
        [write("a.jpg", Rotation::Left)]
    );
}

#[test]
fn a_written_file_is_decoded_again_and_shown_turned_without_a_flash() {
    let mut core = showing();
    press(&mut core, Key::R);
    press(&mut core, Key::Enter);
    // No second write and no further turn while the first is under way.
    assert!(press(&mut core, Key::Enter).is_empty());
    press(&mut core, Key::R);
    assert_eq!(core.view().rotation, Rotation::Right);

    let commands = core.handle(Event::RotationWritten {
        path: p("a.jpg"),
        result: Ok(()),
    });
    assert_eq!(commands, [Command::Decode(p("a.jpg"))]);
    // The old pixels stay up, still turned, until the new ones arrive.
    assert_eq!(core.view().rotation, Rotation::Right);
    assert!(matches!(&core.view().screen, Screen::Image { image, .. } if *image == wide()));

    decoded(&mut core, "a.jpg", tall());
    assert_eq!(core.view().rotation, Rotation::None);
    assert!(matches!(&core.view().screen, Screen::Image { image, .. } if *image == tall()));
    let thumb = core.view().band.iter().find(|t| t.path == p("a.jpg"));
    assert_eq!(thumb.and_then(|t| t.image.clone()), Some(tall()));
    // Turning works again on the new pixels.
    press(&mut core, Key::R);
    assert_eq!(core.view().rotation, Rotation::Right);
}

#[test]
fn a_failed_write_keeps_the_rotation_and_shows_why() {
    let mut core = showing();
    press(&mut core, Key::R);
    press(&mut core, Key::Enter);
    let commands = core.handle(Event::RotationWritten {
        path: p("a.jpg"),
        result: Err("Cannot rotate a.jpg: no".into()),
    });
    assert!(commands.is_empty());
    assert_eq!(core.view().rotation, Rotation::Right);
    assert_eq!(
        core.view().rotation_error.as_deref(),
        Some("Cannot rotate a.jpg: no")
    );
    // It can be tried again.
    assert_eq!(
        press(&mut core, Key::Enter),
        [write("a.jpg", Rotation::Right)]
    );
    press(&mut core, Key::Right);
    assert_eq!(core.view().rotation_error, None);
}

#[test]
fn a_preview_is_not_turned_but_the_rotation_waits_for_the_return() {
    let mut core = showing();
    press(&mut core, Key::R);
    press(&mut core, Key::N);
    core.handle(Event::DirsListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("sub")]),
    });
    assert_eq!(core.view().rotation, Rotation::None);
    press(&mut core, Key::Tab);
    assert_eq!(shown_path(&core), Some(p("a.jpg")));
    assert_eq!(core.view().rotation, Rotation::Right);
}

#[test]
fn the_key_line_lists_rotation() {
    let core = showing();
    let keys = &core.view().key_line.keys;
    assert!(keys.iter().any(|k| k.keys == "L/R" && k.action == "rotate"));
    assert!(
        keys.iter()
            .any(|k| k.keys == "Enter" && k.action == "write rotation")
    );
}
