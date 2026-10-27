//! Seam 1: the presses a click on a key hint makes, and stepping through
//! files by the arrows at the image's edges.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Core, Event, Key, KeyHint, KeyPress, Screen};
use pixpixpix::decode::DecodedImage;

fn presses(keys: &'static str) -> Vec<(std::ops::Range<usize>, Option<KeyPress>)> {
    KeyHint { keys, action: "" }.presses()
}

fn with(key: Key, change: impl Fn(&mut KeyPress)) -> Option<KeyPress> {
    let mut press = KeyPress::plain(key);
    change(&mut press);
    Some(press)
}

#[test]
fn every_key_of_a_hint_is_its_own_press() {
    assert_eq!(
        presses("Left/Esc/Backspace"),
        [
            (0..4, Some(KeyPress::plain(Key::Left))),
            (5..8, Some(KeyPress::plain(Key::Escape))),
            (9..18, Some(KeyPress::plain(Key::Backspace))),
        ]
    );
    assert_eq!(
        presses("+/-"),
        [
            (0..1, Some(KeyPress::plain(Key::Plus))),
            (2..3, Some(KeyPress::plain(Key::Minus))),
        ]
    );
}

#[test]
fn modifiers_carry_on_to_the_keys_after_them() {
    let shift = |p: &mut KeyPress| p.shift = true;
    assert_eq!(
        presses("Shift+Left/Right"),
        [
            (0..10, with(Key::Left, shift)),
            (11..16, with(Key::Right, shift)),
        ]
    );
    assert_eq!(
        presses("./Shift+."),
        [
            (0..1, Some(KeyPress::plain(Key::Period))),
            (2..9, with(Key::Period, shift)),
        ]
    );
    assert_eq!(
        presses("Ctrl+Shift+Z"),
        [(
            0..12,
            with(Key::Z, |p| {
                p.command = true;
                p.shift = true;
            })
        )]
    );
}

#[test]
fn a_name_no_key_has_presses_nothing() {
    assert_eq!(presses("Arrows"), [(0..6, None)]);
}

fn image() -> DecodedImage {
    DecodedImage {
        width: 1,
        height: 1,
        rgba: Arc::from(vec![0; 4]),
    }
}

#[test]
fn the_edge_arrows_step_through_the_files() {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/a.jpg")));
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/a.jpg"),
        result: Ok(image()),
    });
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![PathBuf::from("/p/a.jpg"), PathBuf::from("/p/b.jpg")]),
    });
    // From the column too: the arrows belong to the image.
    core.handle(Event::Key(KeyPress::plain(Key::N)));
    core.handle(Event::Navigate { forward: true });
    assert_eq!(
        core.view().screen,
        Screen::Loading {
            path: PathBuf::from("/p/b.jpg")
        }
    );
    core.handle(Event::Navigate { forward: false });
    assert!(matches!(core.view().screen, Screen::Image { .. }));
}
