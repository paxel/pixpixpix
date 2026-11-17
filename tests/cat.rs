//! Seam 1: the angry cat `Esc` sends off in the image view when it has
//! nothing left to close.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pixpixpix::core::{CAT_SPEED, CAT_TIME, Command, Core, Event, Focus, Key, KeyPress, Points};
use pixpixpix::decode::DecodedImage;

fn image() -> DecodedImage {
    DecodedImage {
        width: 1,
        height: 1,
        rgba: Arc::from(vec![0; 4]),
    }
}

fn press(core: &mut Core, key: Key) -> Vec<Command> {
    core.handle(Event::Key(KeyPress::plain(key)))
}

/// Showing `/p/b.jpg` of `/p/a.jpg` … `/p/c.jpg`.
fn browsing() -> Core {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/b.jpg")));
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/b.jpg"),
        result: Ok(image()),
    });
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(["/p/a.jpg", "/p/b.jpg", "/p/c.jpg"]
            .iter()
            .map(PathBuf::from)
            .collect()),
    });
    core
}

fn offset(core: &Core) -> Option<Points> {
    core.view().cat.map(|cat| cat.offset)
}

fn length(offset: Points) -> f32 {
    offset.x.hypot(offset.y)
}

/// Where the cat of `core` is `after` its first tick.
fn flown(core: &mut Core, start: Instant, after: Duration) -> Option<Points> {
    core.handle(Event::Tick(start));
    core.handle(Event::Tick(start + after));
    offset(core)
}

#[test]
fn esc_with_nothing_to_close_sends_a_cat_from_the_centre() {
    let mut core = browsing();
    assert_eq!(offset(&core), None);
    assert!(press(&mut core, Key::Escape).is_empty());
    assert_eq!(offset(&core), Some(Points::default()));
    assert_eq!(core.view().focus, Focus::Image);
}

#[test]
fn the_cat_flies_in_a_straight_line_and_is_gone_after_a_second() {
    let mut core = browsing();
    press(&mut core, Key::Escape);
    let start = Instant::now();
    // The first tick starts the flight.
    core.handle(Event::Tick(start));
    assert_eq!(offset(&core), Some(Points::default()));
    core.handle(Event::Tick(start + Duration::from_millis(250)));
    let quarter = offset(&core).unwrap();
    core.handle(Event::Tick(start + Duration::from_millis(500)));
    let half = offset(&core).unwrap();
    assert!(length(quarter) > 0.0);
    assert!(
        (half.x - quarter.x * 2.0).abs() < 0.01,
        "{quarter:?} {half:?}"
    );
    assert!(
        (half.y - quarter.y * 2.0).abs() < 0.01,
        "{quarter:?} {half:?}"
    );
    core.handle(Event::Tick(start + CAT_TIME - Duration::from_millis(1)));
    assert!(offset(&core).is_some());
    core.handle(Event::Tick(start + CAT_TIME));
    assert_eq!(offset(&core), None);
    // Later ticks bring nothing back.
    core.handle(Event::Tick(start + CAT_TIME * 2));
    assert_eq!(offset(&core), None);
}

#[test]
fn the_speed_stays_within_its_bounds() {
    let mut core = browsing();
    let start = Instant::now();
    for _ in 0..200 {
        press(&mut core, Key::Escape);
        let half = flown(&mut core, start, Duration::from_millis(500)).unwrap();
        let speed = length(half) * 2.0;
        assert!(
            speed >= CAT_SPEED.start() - 0.5 && speed <= CAT_SPEED.end() + 0.5,
            "{speed}"
        );
    }
}

#[test]
fn a_fixed_seed_fixes_direction_and_speed() {
    let start = Instant::now();
    let fly = |seed: u64| {
        let mut core = browsing();
        core.handle(Event::Seed(seed));
        press(&mut core, Key::Escape);
        flown(&mut core, start, Duration::from_millis(500)).unwrap()
    };
    assert_eq!(fly(7), fly(7));
    assert_ne!(fly(7), fly(8));
    // Zero is a seed like any other.
    assert!(length(fly(0)) > 0.0);
}

#[test]
fn every_esc_sends_a_new_cat_another_way() {
    let mut core = browsing();
    let start = Instant::now();
    press(&mut core, Key::Escape);
    let first = flown(&mut core, start, Duration::from_millis(500)).unwrap();
    // The one on its way gives way to the new one, back at the centre.
    press(&mut core, Key::Escape);
    assert_eq!(offset(&core), Some(Points::default()));
    let second = flown(&mut core, start, Duration::from_millis(500)).unwrap();
    assert_ne!(first, second);
}

#[test]
fn info_and_the_tools_stay_open() {
    let mut core = browsing();
    press(&mut core, Key::I);
    press(&mut core, Key::P);
    press(&mut core, Key::Escape);
    assert!(offset(&core).is_some());
    assert!(core.view().metadata.is_some());
    assert!(core.view().toolbar.is_some());
}

#[test]
fn no_cat_where_esc_closes_something() {
    // The dir column and the band.
    for pane in [Key::N, Key::B] {
        let mut core = browsing();
        press(&mut core, pane);
        press(&mut core, Key::Escape);
        assert_eq!(offset(&core), None, "{pane:?}");
        assert_eq!(core.view().focus, Focus::Image, "{pane:?}");
    }
    // The overview and the bin view.
    for screen in [Key::G, Key::T] {
        let mut core = browsing();
        press(&mut core, screen);
        press(&mut core, Key::Escape);
        assert_eq!(offset(&core), None, "{screen:?}");
        assert!(core.view().overview.is_none(), "{screen:?}");
    }
    // The settings and the About page.
    for screen in [Key::S, Key::F1] {
        let mut core = browsing();
        press(&mut core, screen);
        press(&mut core, Key::Escape);
        assert_eq!(offset(&core), None, "{screen:?}");
        assert!(!core.view().settings_open, "{screen:?}");
        assert!(core.view().about.is_none(), "{screen:?}");
    }
}

#[test]
fn the_second_esc_after_a_pane_is_the_cat() {
    let mut core = browsing();
    press(&mut core, Key::N);
    press(&mut core, Key::Escape);
    assert_eq!(offset(&core), None);
    press(&mut core, Key::Escape);
    assert!(offset(&core).is_some());
}
