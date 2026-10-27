//! The optional native libraries as the About page lists them: whether each
//! is there and where. A helper process loads them, so libmpv never comes into
//! the app's own process.

use std::io;
use std::path::Path;
use std::process::ExitCode;

use crate::core::LibraryStatus;
use crate::helper::{self, Reader, Writer};
use crate::{heif, mpv, turbojpeg};

/// The libraries, their use and how to install them on this system.
const LIBRARIES: [(&str, &str, &str); 3] = if cfg!(target_os = "macos") {
    [
        ("libmpv", "videos", "brew install mpv"),
        ("libheif", "HEIC/HEIF and AVIF", "brew install libheif"),
        (
            "libturbojpeg",
            "lossless JPEG turns",
            "brew install jpeg-turbo",
        ),
    ]
} else {
    [
        (
            "libmpv",
            "videos",
            "libmpv2 on Debian or Ubuntu, mpv-libs on Fedora, mpv on Arch",
        ),
        (
            "libheif",
            "HEIC/HEIF and AVIF",
            "libheif1 with libheif-plugin-libde265 and libheif-plugin-dav1d on Debian or Ubuntu, libheif on Fedora and Arch",
        ),
        (
            "libturbojpeg",
            "lossless JPEG turns",
            "libturbojpeg on Debian or Ubuntu, turbojpeg on Fedora, libjpeg-turbo on Arch",
        ),
    ]
};

/// Looks for the libraries through a helper process. A helper that cannot
/// run marks every library as unknown, with the reason as its hint.
pub fn probe() -> Vec<LibraryStatus> {
    match helper::command(helper::LIBRARIES).and_then(ask) {
        Ok(found) => found,
        Err(error) => LIBRARIES
            .iter()
            .map(|(name, needed_for, _)| LibraryStatus {
                name: (*name).to_owned(),
                needed_for: (*needed_for).to_owned(),
                found: None,
                hint: format!("could not be checked: {error}"),
            })
            .collect(),
    }
}

/// Runs the helper `command` and reads its answers.
fn ask(command: std::process::Command) -> io::Result<Vec<LibraryStatus>> {
    let mut child = helper::spawn(command)?;
    drop(child.stdin.take());
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("no output"))?;
    let mut found = Vec::new();
    while let Some(message) = helper::read_message(&mut stdout)? {
        let mut reader = Reader::new(&message);
        reader.u8();
        let fields: Vec<String> = String::from_utf8_lossy(reader.rest())
            .split(SEPARATOR)
            .map(str::to_owned)
            .collect();
        if let [name, needed_for, path, hint] = fields.as_slice() {
            found.push(LibraryStatus {
                name: name.clone(),
                needed_for: needed_for.clone(),
                found: (!path.is_empty()).then(|| path.clone()),
                hint: hint.clone(),
            });
        }
    }
    child.wait()?;
    Ok(found)
}

/// Between the fields of an answer.
const SEPARATOR: char = '\u{1f}';

/// The helper's side: loads each library and answers where it came from.
pub fn serve() -> ExitCode {
    // Kept loaded until their files are looked up.
    let mpv = mpv::load(&mpv::library_paths());
    let heif = heif::load(heif::LIBRARY_NAMES);
    let turbojpeg = turbojpeg::load(turbojpeg::LIBRARY_NAMES);
    let loaded = [
        mpv.as_ref().map(drop),
        heif.as_ref().map(drop),
        turbojpeg.as_ref().map(drop),
    ];
    let mut stdout = io::stdout();
    for ((name, needed_for, hint), result) in LIBRARIES.iter().zip(loaded) {
        let path = match result {
            Ok(()) => mapped(name).unwrap_or_else(|| "found".to_owned()),
            Err(_) => String::new(),
        };
        let fields = [*name, *needed_for, &path, *hint].join(&SEPARATOR.to_string());
        if helper::write_message(&mut stdout, &Writer::new(0).rest(fields.as_bytes())).is_err() {
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

/// The file of the loaded library whose name starts with `name`, as this
/// process maps it; Linux only.
fn mapped(name: &str) -> Option<String> {
    let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
    maps.lines()
        .filter_map(|line| line.split_whitespace().nth(5))
        .find(|path| {
            Path::new(path)
                .file_name()
                .is_some_and(|f| f.to_string_lossy().starts_with(name))
        })
        .map(str::to_owned)
}
