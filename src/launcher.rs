//! The launcher entry on Linux, written by pixpixpix itself.
//!
//! A Homebrew or tarball install is a binary and nothing else as far as the
//! desktop knows: Homebrew's share directory is not on a session's
//! `XDG_DATA_DIRS`, so the app is not in the menu nor in "Open with", and on
//! Wayland its window shows a generic icon, the compositor finding the icon
//! through the entry named after the window's app id. pixpixpix closes that gap
//! after its first frame: it writes or refreshes the icon and the launcher in
//! the user's XDG data dir, what `packaging/linux/install-icon.sh` does at
//! install. The entry lists every supported type for "Open with" but never
//! makes pixpixpix the default: `mimeapps.list` is not touched.
//!
//! Rules: only an entry carrying the `X-Pixpixpix-Managed` marker is ever
//! overwritten, a hand-written or distro-installed one is left alone; nothing
//! is written when the content is current; every failure is swallowed, a
//! missing menu entry is never worth failing a start over.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::app::APP_ID;

/// The icon as the menu draws it, 256 pixels, by its theme name.
const ICON: &[u8] = include_bytes!("../assets/icon/icon_256.png");
/// The launcher as packaged, with a bare `Exec` this module points at the binary.
const DESKTOP: &str = include_str!("../packaging/linux/io.github.paxel.pixpixpix.desktop");
/// The packaged `Exec` line.
const PACKAGED_EXEC: &str = "Exec=pixpixpix %F";
/// The marker claiming an entry as ours to update.
pub const MARKER: &str = "X-Pixpixpix-Managed=true";

/// Registers icon and launcher for the running binary in the user's data dir;
/// call from a thread of its own. Best effort: errors and a home-less
/// environment are ignored.
pub fn register() {
    let Some(data_home) = data_home() else {
        return;
    };
    let Some(exec) = exec_path() else {
        return;
    };
    if let Ok(true) = register_at(&data_home, &exec) {
        refresh_caches(&data_home);
    }
}

/// `$XDG_DATA_HOME`, else `$HOME/.local/share`.
fn data_home() -> Option<PathBuf> {
    if let Some(x) = std::env::var_os("XDG_DATA_HOME")
        && !x.is_empty()
    {
        return Some(PathBuf::from(x));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
}

/// What the launcher executes: the running executable, through the stable
/// Homebrew symlink when the binary lives in a Cellar.
fn exec_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(stable_brew_path(&exe).unwrap_or(exe))
}

/// The `<prefix>/bin/<name>` symlink for a binary in a Homebrew Cellar,
/// verified to point back at it; none for everything else. `current_exe`
/// resolves into the versioned Cellar directory, which the next
/// `brew upgrade` deletes; a launcher pinned there dies with it. The prefix's
/// `bin` is repointed on every upgrade, so that path lives on.
pub fn stable_brew_path(exe: &Path) -> Option<PathBuf> {
    let prefix = exe
        .ancestors()
        .find(|a| a.file_name() == Some("Cellar".as_ref()))?
        .parent()?;
    let candidate = prefix.join("bin").join(exe.file_name()?);
    (fs::canonicalize(&candidate).ok()? == fs::canonicalize(exe).ok()?).then_some(candidate)
}

/// Writes icon and launcher under `data_home` for `exec`. Whether anything
/// was written; the caches are refreshed only then.
pub fn register_at(data_home: &Path, exec: &Path) -> io::Result<bool> {
    let mut wrote = false;

    let icon_path = data_home.join("icons/hicolor/256x256/apps/pixpixpix.png");
    if fs::read(&icon_path).ok().as_deref() != Some(ICON) {
        write_atomic(&icon_path, ICON)?;
        wrote = true;
    }

    let desktop_path = data_home.join(format!("applications/{APP_ID}.desktop"));
    let desired = desktop_entry(exec);
    match fs::read_to_string(&desktop_path) {
        Ok(current) if !current.lines().any(|line| line == MARKER) => {}
        Ok(current) if current == desired => {}
        _ => {
            write_atomic(&desktop_path, desired.as_bytes())?;
            wrote = true;
        }
    }
    Ok(wrote)
}

/// The packaged launcher with Exec and TryExec pointed at the binary: TryExec
/// hides the entry once the binary is gone instead of failing.
fn desktop_entry(exec: &Path) -> String {
    let exec_str = exec.to_string_lossy();
    DESKTOP.replace(
        PACKAGED_EXEC,
        &format!("TryExec={exec_str}\nExec={} %F", quote_exec(&exec_str)),
    )
}

/// A path for an `Exec=` line: double-quoted with the reserved characters
/// escaped, unless it is plain enough to stand bare; a `%` is doubled either
/// way, as field codes start with it.
fn quote_exec(path: &str) -> String {
    let plain = path
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+' | ':' | '@'));
    if plain {
        return path.to_owned();
    }
    let mut out = String::with_capacity(path.len().saturating_add(2));
    out.push('"');
    for c in path.chars() {
        if matches!(c, '"' | '\\' | '$' | '`') {
            out.push('\\');
        }
        if c == '%' {
            out.push('%');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// Parent directories made, then a same-directory temp file renamed into
/// place, so a crash leaves no half-written launcher.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("path has no parent"))?;
    fs::create_dir_all(dir)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    io::Write::write_all(&mut tmp, bytes)?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Pokes the desktop's caches so the entry shows without a new login. Every
/// tool is optional; absence and failure are both fine.
fn refresh_caches(data_home: &Path) {
    let run = |cmd: &str, args: &[&std::ffi::OsStr]| {
        let _ = std::process::Command::new(cmd)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    };
    let icons = data_home.join("icons/hicolor");
    let apps = data_home.join("applications");
    run(
        "gtk-update-icon-cache",
        &["-f".as_ref(), "-t".as_ref(), icons.as_os_str()],
    );
    run("update-desktop-database", &[apps.as_os_str()]);
    run("kbuildsycoca6", &[]);
}
