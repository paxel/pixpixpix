//! Seams 1 and 3: choosing default apps in the settings, with a fake system
//! backend; the real `xdg-mime` or Launch Services is never called.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pixpixpix::associations::{self, Backend, Opener};
use pixpixpix::core::{Command, Core, DefaultRow, Defaults, Event, Key, KeyPress};
use pixpixpix::decode::DecodedImage;
use pixpixpix::settings::Settings;

const OURS: &str = "io.github.paxel.pixpixpix.desktop";

fn showing() -> Core {
    let (mut core, _) = Core::start(Ok(PathBuf::from("/p/a.jpg")));
    core.handle(Event::Decoded {
        path: PathBuf::from("/p/a.jpg"),
        result: Ok(DecodedImage {
            width: 1,
            height: 1,
            rgba: Arc::from(vec![0; 4]),
        }),
    });
    core.handle(Event::FirstFrameShown);
    core
}

fn row(name: &str, current: Option<&str>) -> DefaultRow {
    DefaultRow {
        name: name.into(),
        video: name == "MP4",
        current: current.map(str::to_owned),
        ours: current == Some(OURS),
    }
}

fn defaults(rows: Vec<DefaultRow>, read_only: bool) -> Defaults {
    Defaults {
        rows,
        names: BTreeMap::new(),
        ours: OURS.into(),
        read_only,
        error: None,
    }
}

/// [`showing`] with the settings open and JPEG opened by gwenview, PNG by
/// pixpixpix, MP4 by nothing.
fn open_settings(read_only: bool) -> Core {
    let mut core = showing();
    core.handle(Event::Key(KeyPress::plain(Key::S)));
    core.handle(Event::DefaultsRead(defaults(
        vec![
            row("JPEG", Some("gwenview.desktop")),
            row("PNG", Some(OURS)),
            row("MP4", None),
        ],
        read_only,
    )));
    core
}

fn set(core: &mut Core, names: &[&str], ours: bool) -> Vec<Command> {
    core.handle(Event::SetDefaults {
        names: names.iter().map(|n| (*n).to_owned()).collect(),
        ours,
    })
}

#[test]
fn opening_the_settings_reads_the_defaults() {
    let mut core = showing();
    assert_eq!(
        core.handle(Event::Key(KeyPress::plain(Key::S))),
        [Command::ReadDefaults { known: Vec::new() }]
    );
}

#[test]
fn choosing_pixpixpix_remembers_the_app_it_replaces() {
    let mut core = open_settings(false);
    let commands = set(&mut core, &["JPEG", "MP4"], true);
    let remembered: BTreeMap<String, String> =
        [("JPEG".to_owned(), "gwenview.desktop".to_owned())].into();
    assert_eq!(
        commands,
        [
            Command::SaveSettings(Settings {
                earlier_defaults: remembered.clone(),
                ..Settings::default()
            }),
            Command::SetDefaults {
                names: vec!["JPEG".into(), "MP4".into()],
                app: OURS.into(),
            },
        ]
    );
    // Once set, the defaults are read again, the remembered app's name too.
    assert_eq!(
        core.handle(Event::DefaultsSet(Ok(()))),
        [Command::ReadDefaults {
            known: vec!["gwenview.desktop".into()]
        }]
    );
    // pixpixpix replacing itself remembers nothing.
    let commands = set(&mut core, &["PNG"], true);
    assert_eq!(
        commands,
        [Command::SetDefaults {
            names: vec!["PNG".into()],
            app: OURS.into(),
        }]
    );
    assert_eq!(core.view().settings.earlier_defaults, remembered);
}

#[test]
fn choosing_the_earlier_app_makes_it_the_default_again() {
    let mut core = open_settings(false);
    set(&mut core, &["JPEG"], true);
    assert_eq!(
        set(&mut core, &["JPEG", "MP4"], false),
        [Command::SetDefaults {
            names: vec!["JPEG".into()],
            app: "gwenview.desktop".into(),
        }]
    );
}

#[test]
fn a_failed_change_says_why_and_reads_again() {
    let mut core = open_settings(false);
    let commands = core.handle(Event::DefaultsSet(Err("xdg-mime failed".into())));
    assert!(matches!(
        commands.as_slice(),
        [Command::ReadDefaults { .. }]
    ));
    assert_eq!(
        core.view().defaults.as_ref().and_then(|d| d.error.clone()),
        Some("xdg-mime failed".into())
    );
}

#[test]
fn read_only_defaults_cannot_be_changed() {
    let mut core = open_settings(true);
    assert!(set(&mut core, &["JPEG"], true).is_empty());
}

/// A backend that knows the defaults it is given and records changes.
struct Fake {
    defaults: RefCell<BTreeMap<String, String>>,
}

impl Backend for Fake {
    fn default_for(&self, id: &str) -> Option<String> {
        self.defaults.borrow().get(id).cloned()
    }

    fn set_default(&self, ids: &[&str], app: &str) -> Result<(), String> {
        for id in ids {
            self.defaults
                .borrow_mut()
                .insert((*id).to_owned(), app.to_owned());
        }
        Ok(())
    }

    fn app_name(&self, app: &str) -> String {
        app.trim_end_matches(".desktop").to_uppercase()
    }

    fn ours(&self) -> String {
        OURS.into()
    }

    fn read_only(&self) -> bool {
        false
    }

    fn open_with(&self, _app: &str, _path: &Path) -> Result<std::process::Command, String> {
        Err("not here".into())
    }
}

#[test]
fn the_defaults_are_read_per_type_with_the_apps_named() {
    let first = |name: &str| associations::named(name).unwrap().ids()[0].to_owned();
    let fake = Fake {
        defaults: RefCell::new(
            [
                (first("JPEG"), "gwenview.desktop".to_owned()),
                (first("PNG"), OURS.to_owned()),
            ]
            .into(),
        ),
    };
    let read = associations::read(&fake, &["vlc.desktop".to_owned()]);
    let jpeg = read.rows.iter().find(|r| r.name == "JPEG").unwrap();
    assert_eq!(jpeg.current.as_deref(), Some("gwenview.desktop"));
    assert!(!jpeg.ours);
    assert!(read.rows.iter().find(|r| r.name == "PNG").unwrap().ours);
    assert_eq!(
        read.rows.iter().filter(|r| r.current.is_none()).count(),
        read.rows.len() - 2
    );
    assert_eq!(
        read.names.get("gwenview.desktop").map(String::as_str),
        Some("GWENVIEW")
    );
    assert_eq!(
        read.names.get("vlc.desktop").map(String::as_str),
        Some("VLC")
    );
    // Setting a type sets every id it has.
    associations::set(&fake, &["HEIC/HEIF".to_owned()], OURS).unwrap();
    for id in associations::named("HEIC/HEIF").unwrap().ids() {
        assert_eq!(fake.default_for(id).as_deref(), Some(OURS), "{id}");
    }
}

#[test]
fn o_opens_with_the_earlier_app_only_while_pixpixpix_is_the_default() {
    assert_eq!(
        associations::opener(Some("gwenview.desktop"), OURS, Some("x")),
        Opener::System
    );
    assert_eq!(associations::opener(None, OURS, None), Opener::System);
    assert_eq!(
        associations::opener(Some(OURS), OURS, Some("gwenview.desktop")),
        Opener::Earlier("gwenview.desktop".into())
    );
    assert_eq!(associations::opener(Some(OURS), OURS, None), Opener::None);
}

#[test]
fn every_supported_extension_has_a_type() {
    for extension in pixpixpix::folder::IMAGE_EXTENSIONS
        .iter()
        .chain(pixpixpix::folder::VIDEO_EXTENSIONS)
        .chain(pixpixpix::folder::HEIF_EXTENSIONS)
        .filter(|e| **e != "mpo")
    {
        let path = PathBuf::from(format!("/p/a.{}", extension.to_uppercase()));
        assert!(associations::type_of(&path).is_some(), "{extension}");
    }
}

#[test]
fn a_desktop_entry_is_named_by_its_main_group() {
    let entry =
        "[Desktop Entry]\nType=Application\nName=Gwenview\n[Desktop Action x]\nName=Other\n";
    assert_eq!(associations::entry_name(entry).as_deref(), Some("Gwenview"));
}
