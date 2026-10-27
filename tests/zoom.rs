//! Seam 1: zoom and pan in the app core, driven by events.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{Core, Event, Key, KeyPress, Placement, Points, Scale, Screen};
use pixpixpix::decode::DecodedImage;
use pixpixpix::rotate::Rotation;

/// 400 × 200, twice as wide as the 200 × 200 viewport: fit is 50 %.
fn wide() -> DecodedImage {
    image(400, 200)
}

fn image(width: u32, height: u32) -> DecodedImage {
    DecodedImage {
        width,
        height,
        rgba: Arc::from(vec![0; 4]),
    }
}

fn p(name: &str) -> PathBuf {
    PathBuf::from(format!("/p/{name}"))
}

fn decoded(core: &mut Core, name: &str) {
    core.handle(Event::Decoded {
        path: p(name),
        result: Ok(wide()),
    });
}

/// Showing `a.jpg` in a 200 × 200 viewport.
fn shown() -> Core {
    let (mut core, _) = Core::start(Ok(p("a.jpg")));
    decoded(&mut core, "a.jpg");
    core.handle(Event::Viewport(Points { x: 200.0, y: 200.0 }));
    core
}

/// Like [`shown`], with the folder listed as `names`.
fn browsing(names: &[&str]) -> Core {
    let mut core = shown();
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(names.iter().map(|n| p(n)).collect()),
    });
    core
}

fn press(core: &mut Core, key: Key) {
    core.handle(Event::Key(KeyPress::plain(key)));
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

#[track_caller]
fn assert_scale(core: &Core, expected: f32) {
    let scale = core.view().zoom.scale;
    assert!(
        matches!(scale, Scale::Exact(s) if close(s, expected)),
        "scale {scale:?}, expected {expected}"
    );
}

#[track_caller]
fn assert_center(core: &Core, x: f32, y: f32) {
    let zoom = core.view().zoom;
    assert!(
        close(zoom.center_x, x) && close(zoom.center_y, y),
        "centre {}/{}, expected {x}/{y}",
        zoom.center_x,
        zoom.center_y
    );
}

#[test]
fn default_view_is_fit() {
    let core = shown();
    assert_eq!(core.view().zoom.scale, Scale::Fit);
    assert_center(&core, 0.5, 0.5);
}

#[test]
fn plus_from_fit_zooms_in_from_the_fit_scale() {
    let mut core = shown();
    press(&mut core, Key::Plus);
    assert_scale(&core, 0.625);
}

#[test]
fn plus_typed_with_shift_still_zooms() {
    let mut core = shown();
    core.handle(Event::Key(KeyPress {
        shift: true,
        ..KeyPress::plain(Key::Plus)
    }));
    assert_scale(&core, 0.625);
}

#[test]
fn minus_zooms_out() {
    let mut core = shown();
    press(&mut core, Key::Num1);
    press(&mut core, Key::Minus);
    assert_scale(&core, 0.8);
}

#[test]
fn one_shows_100_percent_and_zero_fits_again() {
    let mut core = shown();
    press(&mut core, Key::Num1);
    assert_scale(&core, 1.0);
    press(&mut core, Key::Num0);
    assert_eq!(core.view().zoom.scale, Scale::Fit);
}

#[test]
fn zoom_needs_a_viewport() {
    let (mut core, _) = Core::start(Ok(p("a.jpg")));
    decoded(&mut core, "a.jpg");
    press(&mut core, Key::Plus);
    assert_eq!(core.view().zoom.scale, Scale::Fit);
}

#[test]
fn zoom_needs_an_image() {
    let (mut core, _) = Core::start(Ok(p("a.jpg")));
    core.handle(Event::Viewport(Points { x: 200.0, y: 200.0 }));
    press(&mut core, Key::Plus);
    assert_eq!(core.view().zoom.scale, Scale::Fit);
}

#[test]
fn wheel_zooms_toward_the_pointer() {
    let mut core = shown();
    press(&mut core, Key::Num1);
    // Doubles the scale with the pointer at the viewport's right edge.
    core.handle(Event::Wheel {
        delta: 200.0 * std::f32::consts::LN_2,
        pointer: Points { x: 100.0, y: 0.0 },
    });
    assert_scale(&core, 2.0);
    // Image x 300 stays under the pointer: 300 - 100 / 2 = 250 of 400.
    assert_center(&core, 0.625, 0.5);
}

#[test]
fn wheel_down_zooms_out() {
    let mut core = shown();
    press(&mut core, Key::Num1);
    core.handle(Event::Wheel {
        delta: -200.0 * std::f32::consts::LN_2,
        pointer: Points::default(),
    });
    assert_scale(&core, 0.5);
}

#[test]
fn dragging_pans_the_image() {
    let mut core = shown();
    press(&mut core, Key::Num1);
    // Dragging left by 40 points reveals 40 more pixels on the right.
    core.handle(Event::Pan(Points { x: -40.0, y: 0.0 }));
    assert_center(&core, 0.6, 0.5);
}

#[test]
fn panning_stops_at_the_image_edge() {
    let mut core = shown();
    press(&mut core, Key::Num1);
    core.handle(Event::Pan(Points {
        x: -10_000.0,
        y: 10_000.0,
    }));
    assert_center(&core, 1.0, 0.0);
}

#[test]
fn panning_in_fit_does_nothing() {
    let mut core = shown();
    core.handle(Event::Pan(Points { x: -40.0, y: 0.0 }));
    assert_eq!(core.view().zoom.scale, Scale::Fit);
    assert_center(&core, 0.5, 0.5);
}

#[test]
fn switching_to_a_cached_image_keeps_zoom_and_position() {
    let mut core = browsing(&["a.jpg", "b.jpg"]);
    decoded(&mut core, "b.jpg");
    press(&mut core, Key::Num1);
    core.handle(Event::Pan(Points { x: -40.0, y: 0.0 }));
    press(&mut core, Key::Right);
    assert!(matches!(&core.view().screen, Screen::Image { path, .. } if *path == p("b.jpg")));
    assert_scale(&core, 1.0);
    assert_center(&core, 0.6, 0.5);
}

#[test]
fn switching_to_an_image_still_loading_keeps_zoom_and_position() {
    let mut core = browsing(&["a.jpg", "b.jpg"]);
    press(&mut core, Key::Num1);
    core.handle(Event::Pan(Points { x: -40.0, y: 0.0 }));
    press(&mut core, Key::Right);
    assert!(matches!(core.view().screen, Screen::Loading { .. }));
    decoded(&mut core, "b.jpg");
    assert!(matches!(core.view().screen, Screen::Image { .. }));
    assert_scale(&core, 1.0);
    assert_center(&core, 0.6, 0.5);
}

#[test]
fn fit_places_the_image_centred_and_never_upscaled() {
    let core = shown();
    let viewport = Points { x: 200.0, y: 200.0 };
    let zoom = core.view().zoom;
    assert_eq!(
        zoom.placement(&wide(), Rotation::None, viewport),
        Placement {
            left: 0.0,
            top: 50.0,
            width: 200.0,
            height: 100.0
        }
    );
    assert_eq!(
        zoom.placement(&image(20, 10), Rotation::None, viewport),
        Placement {
            left: 90.0,
            top: 95.0,
            width: 20.0,
            height: 10.0
        }
    );
}

#[test]
fn zoomed_placement_puts_the_centre_point_in_the_middle() {
    let mut core = shown();
    press(&mut core, Key::Num1);
    core.handle(Event::Pan(Points { x: -50.0, y: 0.0 }));
    // Image x 250 sits at the viewport centre 100; the height fits and is
    // centred.
    assert_eq!(
        core.view()
            .zoom
            .placement(&wide(), Rotation::None, Points { x: 200.0, y: 200.0 }),
        Placement {
            left: -150.0,
            top: 0.0,
            width: 400.0,
            height: 200.0
        }
    );
}

fn shift(core: &mut Core, key: Key) {
    core.handle(Event::Key(KeyPress {
        shift: true,
        ..KeyPress::plain(key)
    }));
}

fn twice_the_size() -> Core {
    let mut core = shown();
    press(&mut core, Key::Num1);
    core.handle(Event::Wheel {
        delta: 200.0 * std::f32::consts::LN_2,
        pointer: Points::default(),
    });
    core
}

#[test]
fn shift_arrows_move_the_zoomed_picture_a_tenth_of_the_window() {
    let mut core = twice_the_size();
    // 800 × 400 points in a 200 × 200 window: a tenth is 20 points.
    shift(&mut core, Key::Right);
    assert_center(&core, 0.525, 0.5);
    shift(&mut core, Key::Down);
    assert_center(&core, 0.525, 0.55);
    shift(&mut core, Key::Left);
    shift(&mut core, Key::Up);
    assert_center(&core, 0.5, 0.5);
}

#[test]
fn shift_arrows_stop_at_the_picture_edges() {
    let mut core = twice_the_size();
    for _ in 0..100 {
        shift(&mut core, Key::Left);
        shift(&mut core, Key::Up);
    }
    assert_center(&core, 0.0, 0.0);
}

#[test]
fn shift_arrows_change_nothing_at_fit_size() {
    let mut core = browsing(&["a.jpg", "b.jpg"]);
    decoded(&mut core, "b.jpg");
    for key in [Key::Left, Key::Right, Key::Up, Key::Down] {
        shift(&mut core, key);
    }
    assert_eq!(core.view().zoom.scale, Scale::Fit);
    assert_center(&core, 0.5, 0.5);
    assert!(matches!(&core.view().screen, Screen::Image { path, .. } if *path == p("a.jpg")));
}

#[test]
fn the_move_hint_shows_only_while_the_picture_is_larger_than_the_window() {
    let hint = |core: &Core| {
        core.view()
            .key_line
            .keys
            .iter()
            .any(|k| k.keys == "Shift+Left/Right/Up/Down" && k.action == "move")
    };
    let mut core = shown();
    assert!(!hint(&core));
    press(&mut core, Key::Num1);
    assert!(hint(&core));
    press(&mut core, Key::Num0);
    assert!(!hint(&core));
}
