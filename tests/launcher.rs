//! Seam 3: the launcher entry and icon, written into a temporary data dir.

use std::fs;
use std::path::Path;

use pixpixpix::launcher::{self, MARKER};

const ENTRY: &str = "applications/io.github.paxel.pixpixpix.desktop";
const ICON: &str = "icons/hicolor/256x256/apps/pixpixpix.png";

/// The written launcher entry, empty when there is none.
fn entry(dir: &Path) -> String {
    fs::read_to_string(dir.join(ENTRY)).unwrap_or_default()
}

#[test]
fn a_fresh_register_writes_the_icon_and_a_marked_launcher() {
    let dir = tempfile::tempdir().unwrap();
    assert!(launcher::register_at(dir.path(), Path::new("/opt/brew/bin/pixpixpix")).unwrap());
    assert_eq!(
        fs::read(dir.path().join(ICON)).unwrap(),
        fs::read("assets/icon/icon_256.png").unwrap()
    );
    let desktop = entry(dir.path());
    assert!(desktop.contains("\nTryExec=/opt/brew/bin/pixpixpix\n"));
    assert!(desktop.contains("\nExec=/opt/brew/bin/pixpixpix %F\n"));
    assert!(desktop.contains("\nIcon=pixpixpix\n"));
    assert!(desktop.contains("\nStartupWMClass=io.github.paxel.pixpixpix\n"));
    assert!(desktop.lines().any(|line| line == MARKER));
}

#[test]
fn the_launcher_offers_every_supported_type_but_claims_no_default() {
    let dir = tempfile::tempdir().unwrap();
    launcher::register_at(dir.path(), Path::new("/usr/bin/pixpixpix")).unwrap();
    let desktop = entry(dir.path());
    let types = desktop
        .lines()
        .find_map(|line| line.strip_prefix("MimeType="))
        .unwrap();
    for wanted in [
        "image/jpeg",
        "image/png",
        "image/gif",
        "image/webp",
        "image/bmp",
        "image/tiff",
        "image/heif",
        "image/avif",
        "video/mp4",
        "video/quicktime",
        "video/x-matroska",
        "video/webm",
        "video/mp2t",
        "video/mpeg",
    ] {
        assert!(types.split(';').any(|t| t == wanted), "{wanted} missing");
    }
    // Only the entry and the icon: no mimeapps.list, so never the default.
    assert!(!dir.path().join("applications/mimeapps.list").exists());
    let mut written: Vec<_> = walk(dir.path());
    written.sort();
    assert_eq!(written, [ENTRY, ICON]);
}

/// Every file below `root`, relative to it.
fn walk(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for path in fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
        {
            if path.is_dir() {
                dirs.push(path);
            } else if let Ok(relative) = path.strip_prefix(root) {
                files.push(relative.to_string_lossy().into_owned());
            }
        }
    }
    files
}

#[test]
fn current_content_is_not_written_again_but_a_moved_binary_is() {
    let dir = tempfile::tempdir().unwrap();
    let exec = Path::new("/usr/local/bin/pixpixpix");
    assert!(launcher::register_at(dir.path(), exec).unwrap());
    let modified = fs::metadata(dir.path().join(ENTRY))
        .unwrap()
        .modified()
        .unwrap();
    assert!(!launcher::register_at(dir.path(), exec).unwrap());
    assert_eq!(
        fs::metadata(dir.path().join(ENTRY))
            .unwrap()
            .modified()
            .unwrap(),
        modified
    );
    assert!(launcher::register_at(dir.path(), Path::new("/new/pixpixpix")).unwrap());
    assert!(entry(dir.path()).contains("\nExec=/new/pixpixpix %F\n"));
}

#[test]
fn a_changed_icon_is_written_again() {
    let dir = tempfile::tempdir().unwrap();
    let exec = Path::new("/usr/bin/pixpixpix");
    launcher::register_at(dir.path(), exec).unwrap();
    fs::write(dir.path().join(ICON), b"old").unwrap();
    assert!(launcher::register_at(dir.path(), exec).unwrap());
    assert_ne!(fs::read(dir.path().join(ICON)).unwrap(), b"old");
}

#[test]
fn a_launcher_without_the_marker_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(ENTRY);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let foreign = "[Desktop Entry]\nName=my pixpixpix\nExec=/home/me/custom %F\n";
    fs::write(&path, foreign).unwrap();
    launcher::register_at(dir.path(), Path::new("/opt/pixpixpix")).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), foreign);
}

#[test]
fn an_outdated_marked_launcher_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(ENTRY);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, format!("[Desktop Entry]\nExec=/gone %F\n{MARKER}\n")).unwrap();
    assert!(launcher::register_at(dir.path(), Path::new("/usr/bin/pixpixpix")).unwrap());
    assert!(entry(dir.path()).contains("\nExec=/usr/bin/pixpixpix %F\n"));
}

#[test]
fn exec_paths_with_specials_are_quoted() {
    let dir = tempfile::tempdir().unwrap();
    launcher::register_at(dir.path(), Path::new("/home/me/My Apps/100%/pix$x")).unwrap();
    let desktop = entry(dir.path());
    assert!(desktop.contains("\nExec=\"/home/me/My Apps/100%%/pix\\$x\" %F\n"));
    assert!(desktop.contains("\nTryExec=/home/me/My Apps/100%/pix$x\n"));
}

/// The formula links `bin/pixpixpix` to the binary in `libexec`; a binary
/// running from the versioned Cellar registers the prefix's stable `bin` link,
/// which a `brew upgrade` keeps alive.
#[cfg(unix)]
#[test]
fn a_cellar_binary_registers_the_stable_bin_symlink() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let keg = dir.path().join("Cellar/pixpixpix/0.1.0");
    fs::create_dir_all(keg.join("libexec")).unwrap();
    fs::create_dir_all(keg.join("bin")).unwrap();
    let exe = keg.join("libexec/pixpixpix");
    fs::write(&exe, b"binary").unwrap();
    symlink("../libexec/pixpixpix", keg.join("bin/pixpixpix")).unwrap();
    fs::create_dir_all(dir.path().join("bin")).unwrap();
    symlink(
        "../Cellar/pixpixpix/0.1.0/bin/pixpixpix",
        dir.path().join("bin/pixpixpix"),
    )
    .unwrap();
    assert_eq!(
        launcher::stable_brew_path(&exe),
        Some(dir.path().join("bin/pixpixpix"))
    );

    // A bin link naming a different binary is not claimed.
    let other = dir.path().join("Cellar/other/1.0/libexec");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("pixpixpix"), b"other").unwrap();
    assert_eq!(launcher::stable_brew_path(&other.join("pixpixpix")), None);

    // No Cellar in the path: the binary registers as itself.
    assert_eq!(
        launcher::stable_brew_path(Path::new("/usr/local/bin/pixpixpix")),
        None
    );
}

#[test]
fn a_flatpak_sandbox_is_told_apart_by_its_info_file() {
    // Inside a Flatpak the launcher is the exported one, so nothing is written
    // into the host's data dir.
    let root = tempfile::tempdir().unwrap();
    assert!(!launcher::in_flatpak(root.path()));
    fs::write(root.path().join(".flatpak-info"), "[Application]\n").unwrap();
    assert!(launcher::in_flatpak(root.path()));
}
