//! Which app opens each file type by default, read and changed through the
//! system: `xdg-mime` on Linux, Launch Services on macOS. In the Flatpak the
//! defaults can only be read. pixpixpix becomes the default only by the user's
//! choice in the settings; installing never makes it one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::app::APP_ID;
use crate::core::{DefaultRow, Defaults};

/// A file type the settings list, with the system's names for it.
pub struct FileType {
    /// What the settings call it.
    pub name: &'static str,
    pub video: bool,
    /// The MIME types on Linux.
    pub mime: &'static [&'static str],
    /// The uniform type identifiers on macOS.
    pub uti: &'static [&'static str],
    /// The file name extensions, lower case.
    pub extensions: &'static [&'static str],
}

impl FileType {
    /// The system's identifiers of the type.
    pub fn ids(&self) -> &'static [&'static str] {
        if cfg!(target_os = "macos") {
            self.uti
        } else {
            self.mime
        }
    }
}

const fn image(
    name: &'static str,
    mime: &'static [&'static str],
    uti: &'static [&'static str],
    extensions: &'static [&'static str],
) -> FileType {
    FileType {
        name,
        video: false,
        mime,
        uti,
        extensions,
    }
}

const fn video(
    name: &'static str,
    mime: &'static [&'static str],
    uti: &'static [&'static str],
    extensions: &'static [&'static str],
) -> FileType {
    FileType {
        name,
        video: true,
        mime,
        uti,
        extensions,
    }
}

/// The types pixpixpix opens, images first.
pub const TYPES: &[FileType] = &[
    image(
        "JPEG",
        &["image/jpeg"],
        &["public.jpeg"],
        &["jpg", "jpeg", "jpe"],
    ),
    image("PNG", &["image/png"], &["public.png"], &["png"]),
    image("GIF", &["image/gif"], &["com.compuserve.gif"], &["gif"]),
    image(
        "WebP",
        &["image/webp"],
        &["org.webmproject.webp"],
        &["webp"],
    ),
    image("BMP", &["image/bmp"], &["com.microsoft.bmp"], &["bmp"]),
    image("TIFF", &["image/tiff"], &["public.tiff"], &["tif", "tiff"]),
    image(
        "HEIC/HEIF",
        &["image/heic", "image/heif"],
        &["public.heic", "public.heif"],
        &["heic", "heif", "hif"],
    ),
    image("AVIF", &["image/avif"], &["public.avif"], &["avif"]),
    video("MP4", &["video/mp4"], &["public.mpeg-4"], &["mp4"]),
    video("M4V", &["video/x-m4v"], &["com.apple.m4v-video"], &["m4v"]),
    video(
        "QuickTime",
        &["video/quicktime"],
        &["com.apple.quicktime-movie"],
        &["mov"],
    ),
    video(
        "Matroska",
        &["video/x-matroska"],
        &["org.matroska.mkv"],
        &["mkv"],
    ),
    video(
        "WebM",
        &["video/webm"],
        &["org.webmproject.webm"],
        &["webm"],
    ),
    video(
        "AVI",
        &["video/vnd.avi", "video/x-msvideo"],
        &["public.avi"],
        &["avi"],
    ),
    video(
        "MPEG-TS",
        &["video/mp2t"],
        &["public.avchd-mpeg-2-transport-stream"],
        &["mts", "m2ts"],
    ),
    video("3GP", &["video/3gpp"], &["public.3gpp"], &["3gp"]),
    video(
        "WMV",
        &["video/x-ms-wmv"],
        &["com.microsoft.windows-media-wmv"],
        &["wmv"],
    ),
    video(
        "FLV",
        &["video/x-flv"],
        &["com.adobe.flash.video"],
        &["flv"],
    ),
    video("MPEG", &["video/mpeg"], &["public.mpeg"], &["mpg"]),
];

/// The type of the file at `path`, by its extension.
pub fn type_of(path: &Path) -> Option<&'static FileType> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    TYPES
        .iter()
        .find(|t| t.extensions.contains(&extension.as_str()))
}

/// The type the settings call `name`.
pub fn named(name: &str) -> Option<&'static FileType> {
    TYPES.iter().find(|t| t.name == name)
}

/// How the system keeps the default apps.
pub trait Backend {
    /// The id of the default app for the type `id`, if there is one.
    fn default_for(&self, id: &str) -> Option<String>;
    /// Makes `app` the default for the types `ids`.
    fn set_default(&self, ids: &[&str], app: &str) -> Result<(), String>;
    /// What to call the app `app`.
    fn app_name(&self, app: &str) -> String;
    /// pixpixpix's own id.
    fn ours(&self) -> String;
    /// The defaults can be read but not changed.
    fn read_only(&self) -> bool;
    /// Opens `path` in the app `app`.
    fn open_with(&self, app: &str, path: &Path) -> Result<Command, String>;
}

/// The defaults of every type as the settings show them, the names of the
/// apps in `known` too.
pub fn read(backend: &dyn Backend, known: &[String]) -> Defaults {
    let ours = backend.ours();
    let rows: Vec<DefaultRow> = TYPES
        .iter()
        .map(|file_type| {
            let current = file_type
                .ids()
                .first()
                .and_then(|id| backend.default_for(id));
            DefaultRow {
                name: file_type.name.to_owned(),
                video: file_type.video,
                ours: current.as_deref() == Some(ours.as_str()),
                current,
            }
        })
        .collect();
    let names: BTreeMap<String, String> = rows
        .iter()
        .filter_map(|row| row.current.clone())
        .chain(known.iter().cloned())
        .map(|app| {
            let name = backend.app_name(&app);
            (app, name)
        })
        .collect();
    Defaults {
        rows,
        names,
        ours,
        read_only: backend.read_only(),
        error: None,
    }
}

/// Makes `app` the default for the types the settings call `names`.
pub fn set(backend: &dyn Backend, names: &[String], app: &str) -> Result<(), String> {
    let ids: Vec<&str> = names
        .iter()
        .filter_map(|name| named(name))
        .flat_map(|t| t.ids().iter().copied())
        .collect();
    backend.set_default(&ids, app)
}

/// What opens a file `o` asks for when the settings name no app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opener {
    /// The system default, which is not pixpixpix.
    System,
    /// The app that was the default before pixpixpix became it.
    Earlier(String),
    /// pixpixpix is the default and no other app is known.
    None,
}

/// The opener for a file whose type has `default` as the default app, with
/// `earlier` remembered from before pixpixpix became it.
pub fn opener(default: Option<&str>, ours: &str, earlier: Option<&str>) -> Opener {
    match (default == Some(ours), earlier) {
        (false, _) => Opener::System,
        (true, Some(earlier)) => Opener::Earlier(earlier.to_owned()),
        (true, None) => Opener::None,
    }
}

/// The system's backend.
pub fn system() -> Box<dyn Backend> {
    #[cfg(target_os = "macos")]
    {
        Box::new(crate::macos::LaunchServices)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(XdgMime {
            flatpak: std::env::var_os("FLATPAK_ID").is_some(),
        })
    }
}

/// `xdg-mime`, and the desktop entries of the XDG data directories.
pub struct XdgMime {
    pub flatpak: bool,
}

impl Backend for XdgMime {
    fn default_for(&self, id: &str) -> Option<String> {
        let output = Command::new("xdg-mime")
            .args(["query", "default", id])
            .output()
            .ok()?;
        let app = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (output.status.success() && !app.is_empty()).then_some(app)
    }

    fn set_default(&self, ids: &[&str], app: &str) -> Result<(), String> {
        let status = Command::new("xdg-mime")
            .arg("default")
            .arg(app)
            .args(ids)
            .status()
            .map_err(|e| format!("Cannot run xdg-mime: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("xdg-mime failed: {status}"))
        }
    }

    fn app_name(&self, app: &str) -> String {
        desktop_entry(app)
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|entry| entry_name(&entry))
            .unwrap_or_else(|| app.trim_end_matches(".desktop").to_owned())
    }

    fn ours(&self) -> String {
        format!("{APP_ID}.desktop")
    }

    fn read_only(&self) -> bool {
        self.flatpak
    }

    fn open_with(&self, app: &str, path: &Path) -> Result<Command, String> {
        let entry = desktop_entry(app).ok_or_else(|| format!("{app} is not installed"))?;
        let mut command = Command::new("gio");
        command.arg("launch").arg(entry).arg(path);
        Ok(command)
    }
}

/// The file of the desktop entry `app` in the XDG data directories.
fn desktop_entry(app: &str) -> Option<PathBuf> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|home| home.join(".local/share")));
    let dirs =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned());
    home.into_iter()
        .chain(dirs.split(':').map(PathBuf::from))
        .map(|dir| dir.join("applications").join(app))
        .find(|path| path.is_file())
}

/// The `Name` of a desktop entry's main group.
pub fn entry_name(entry: &str) -> Option<String> {
    entry
        .lines()
        .skip_while(|line| line.trim() != "[Desktop Entry]")
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .find_map(|line| line.strip_prefix("Name="))
        .map(|name| name.trim().to_owned())
}
