//! Seam 1: the metadata view and the capture date edit, driven by events.

use std::path::PathBuf;
use std::sync::Arc;

use pixpixpix::core::{
    Command, Core, DATE_FORMAT_ERROR, Event, Key, KeyPress, MetadataView, Screen,
};
use pixpixpix::decode::DecodedImage;
use pixpixpix::metadata::{Field, Metadata};

fn p(name: &str) -> PathBuf {
    PathBuf::from(format!("/p/{name}"))
}

fn image() -> DecodedImage {
    DecodedImage {
        width: 4,
        height: 3,
        rgba: Arc::from(vec![9; 48]),
    }
}

fn key(core: &mut Core, key: Key) -> Vec<Command> {
    core.handle(Event::Key(KeyPress::plain(key)))
}

/// Showing a.jpg of a listed folder with b.jpg.
fn browsing() -> Core {
    let (mut core, _) = Core::start(Ok(p("a.jpg")));
    core.handle(Event::Decoded {
        path: p("a.jpg"),
        result: Ok(image()),
    });
    core.handle(Event::FirstFrameShown);
    core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("a.jpg"), p("b.jpg")]),
    });
    core
}

fn field(label: &'static str, value: &str) -> Field {
    Field {
        label,
        value: value.to_owned(),
    }
}

fn read(name: &str, captured: Option<&str>) -> Metadata {
    Metadata {
        file: vec![field("File", name)],
        captured: captured.map(str::to_owned),
        exif: vec![field("Camera", "Pixcam")],
    }
}

/// The metadata view open on a.jpg, its date read.
fn open() -> Core {
    let mut core = browsing();
    assert_eq!(key(&mut core, Key::I), [Command::ReadMetadata(p("a.jpg"))]);
    core.handle(Event::MetadataRead {
        path: p("a.jpg"),
        result: Ok(read("a.jpg", Some("2019-06-07 08:09:10"))),
    });
    core
}

/// The metadata view; a closed one shows as an error no test expects.
fn shown(core: &Core) -> MetadataView {
    core.view()
        .metadata
        .clone()
        .unwrap_or_else(|| MetadataView {
            reading: false,
            file: Vec::new(),
            date: None,
            exif: Vec::new(),
            error: Some("closed".into()),
            writing: false,
        })
}

#[test]
fn i_toggles_the_metadata_view_and_reads_the_current_file() {
    let mut core = browsing();
    assert_eq!(core.view().metadata, None);
    key(&mut core, Key::I);
    assert!(shown(&core).reading);
    core.handle(Event::MetadataRead {
        path: p("a.jpg"),
        result: Ok(read("a.jpg", Some("2019-06-07 08:09:10"))),
    });
    assert_eq!(
        core.view().metadata,
        Some(MetadataView {
            reading: false,
            file: vec![field("File", "a.jpg"), field("Image size", "4 × 3")],
            date: Some("2019-06-07 08:09:10".into()),
            exif: vec![field("Camera", "Pixcam")],
            error: None,
            writing: false,
        })
    );
    assert!(key(&mut core, Key::I).is_empty());
    assert_eq!(core.view().metadata, None);
}

#[test]
fn the_key_line_lists_i() {
    let core = browsing();
    assert!(
        core.view()
            .key_line
            .keys
            .iter()
            .any(|k| k.keys == "I" && k.action == "info")
    );
}

#[test]
fn moving_on_reads_the_next_file_and_ignores_the_old_answer() {
    let mut core = open();
    let commands = key(&mut core, Key::Right);
    assert!(commands.contains(&Command::ReadMetadata(p("b.jpg"))));
    assert!(shown(&core).reading);
    core.handle(Event::MetadataRead {
        path: p("a.jpg"),
        result: Ok(read("a.jpg", None)),
    });
    assert!(shown(&core).reading);
}

#[test]
fn a_file_without_a_capture_date_has_no_date_field() {
    let mut core = browsing();
    key(&mut core, Key::I);
    core.handle(Event::MetadataRead {
        path: p("a.jpg"),
        result: Ok(read("a.jpg", None)),
    });
    assert_eq!(shown(&core).date, None);
    core.handle(Event::DateEdited("2021-12-24 18:30:00".into()));
    assert!(core.handle(Event::DateEntered).is_empty());
}

#[test]
fn a_failed_read_is_shown() {
    let mut core = browsing();
    key(&mut core, Key::I);
    core.handle(Event::MetadataRead {
        path: p("a.jpg"),
        result: Err("Cannot read a.jpg: gone".into()),
    });
    assert_eq!(
        shown(&core).error.as_deref(),
        Some("Cannot read a.jpg: gone")
    );
    assert!(!shown(&core).reading);
}

#[test]
fn return_writes_a_valid_date_as_exif_stores_it() {
    let mut core = open();
    core.handle(Event::DateEdited("2021-12-24 18:30:00".into()));
    assert_eq!(shown(&core).date.as_deref(), Some("2021-12-24 18:30:00"));
    assert_eq!(
        core.handle(Event::DateEntered),
        [Command::WriteDate {
            path: p("a.jpg"),
            date: "2021:12:24 18:30:00".into()
        }]
    );
    assert!(shown(&core).writing);
    // A second Return while writing writes nothing more.
    assert!(core.handle(Event::DateEntered).is_empty());
}

#[test]
fn an_invalid_date_is_refused_with_an_error() {
    let mut core = open();
    core.handle(Event::DateEdited("2021-13-24".into()));
    assert!(core.handle(Event::DateEntered).is_empty());
    assert_eq!(shown(&core).error.as_deref(), Some(DATE_FORMAT_ERROR));
    core.handle(Event::DateEdited("2021-12-24 18:30:00".into()));
    assert_eq!(shown(&core).error, None);
}

#[test]
fn an_unchanged_date_is_not_written() {
    let mut core = open();
    assert!(core.handle(Event::DateEntered).is_empty());
    core.handle(Event::DateEdited("2019-06-07 08:09:10".into()));
    assert!(core.handle(Event::DateEntered).is_empty());
}

#[test]
fn a_written_date_is_read_back_without_decoding_or_flicker() {
    let mut core = open();
    core.handle(Event::DateEdited("2021-12-24 18:30:00".into()));
    core.handle(Event::DateEntered);
    let commands = core.handle(Event::DateWritten {
        path: p("a.jpg"),
        result: Ok(()),
    });
    assert_eq!(commands, [Command::ReadMetadata(p("a.jpg"))]);
    // The typed date stays until the new one is read.
    assert_eq!(shown(&core).date.as_deref(), Some("2021-12-24 18:30:00"));
    // The watcher reports the rewritten file.
    let commands = core.handle(Event::FolderListed {
        dir: PathBuf::from("/p"),
        result: Ok(vec![p("a.jpg"), p("b.jpg")]),
    });
    assert!(!commands.contains(&Command::Decode(p("a.jpg"))));
    assert_eq!(
        core.view().screen,
        Screen::Image {
            path: p("a.jpg"),
            image: image()
        }
    );
    core.handle(Event::MetadataRead {
        path: p("a.jpg"),
        result: Ok(read("a.jpg", Some("2021-12-24 18:30:00"))),
    });
    assert_eq!(shown(&core).date.as_deref(), Some("2021-12-24 18:30:00"));
    assert!(!shown(&core).writing);
}

#[test]
fn a_failed_write_keeps_the_typed_date_and_says_why() {
    let mut core = open();
    core.handle(Event::DateEdited("2021-12-24 18:30:00".into()));
    core.handle(Event::DateEntered);
    let commands = core.handle(Event::DateWritten {
        path: p("a.jpg"),
        result: Err("Cannot change the date of a.jpg: it has no capture date".into()),
    });
    assert!(commands.is_empty());
    let view = shown(&core);
    assert_eq!(view.date.as_deref(), Some("2021-12-24 18:30:00"));
    assert!(!view.writing);
    assert_eq!(
        view.error.as_deref(),
        Some("Cannot change the date of a.jpg: it has no capture date")
    );
}

#[test]
fn the_overview_hides_the_metadata_view() {
    let mut core = open();
    key(&mut core, Key::G);
    assert_eq!(core.view().metadata, None);
    key(&mut core, Key::G);
    assert!(core.view().metadata.is_some());
}
