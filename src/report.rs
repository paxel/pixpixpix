//! The text an error's copy button puts on the clipboard: the message and
//! what a search or a bug report needs besides, the file, the version, the
//! system and how pixpixpix was installed.

use std::path::Path;

/// The report of the error `message` about the file at `path`, for this
/// executable.
pub fn report(message: &str, path: Option<&Path>) -> String {
    let exe = std::env::current_exe().unwrap_or_default();
    let flatpak = std::env::var_os("FLATPAK_ID").is_some();
    report_for(message, path, install_method(&exe, flatpak))
}

/// [`report`] with the install method given.
pub fn report_for(message: &str, path: Option<&Path>, installed: &str) -> String {
    let mut text = format!("{message}\n");
    if let Some(path) = path {
        text.push_str(&format!("File: {}\n", path.display()));
    }
    text.push_str(&format!(
        "pixpixpix {} on {} {}, installed as {installed}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    ));
    text
}

/// How the executable at `exe` was installed, `flatpak` telling whether it
/// runs in a Flatpak sandbox.
pub fn install_method(exe: &Path, flatpak: bool) -> &'static str {
    let path = exe.to_string_lossy();
    if flatpak {
        "Flatpak"
    } else if path.contains(".app/Contents/MacOS/") {
        "macOS app"
    } else if ["/Cellar/", "/homebrew/", "/linuxbrew/"]
        .iter()
        .any(|part| path.contains(part))
    {
        "Homebrew"
    } else {
        "release archive or own build"
    }
}
