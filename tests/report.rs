//! Seam 3: the report an error's copy button puts on the clipboard.

use std::path::Path;

use pixpixpix::report;

#[test]
fn the_report_holds_the_message_the_file_the_version_and_the_install() {
    let text = report::report_for("broken", Some(Path::new("/p/a.jpg")), "Homebrew");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "broken");
    assert_eq!(lines[1], "File: /p/a.jpg");
    let version = format!("pixpixpix {} on ", env!("CARGO_PKG_VERSION"));
    assert!(lines[2].starts_with(&version), "{}", lines[2]);
    assert!(lines[2].contains(std::env::consts::OS), "{}", lines[2]);
    assert!(
        lines[2].ends_with(", installed as Homebrew"),
        "{}",
        lines[2]
    );
    // Without a file there is no file line.
    assert_eq!(report::report_for("x", None, "Flatpak").lines().count(), 2);
}

#[test]
fn the_install_method_comes_from_where_the_executable_lives() {
    let method = |exe: &str, flatpak| report::install_method(Path::new(exe), flatpak);
    assert_eq!(method("/app/bin/pixpixpix", true), "Flatpak");
    assert_eq!(
        method(
            "/Applications/pixpixpix.app/Contents/MacOS/pixpixpix",
            false
        ),
        "macOS app"
    );
    assert_eq!(
        method(
            "/home/linuxbrew/.linuxbrew/Cellar/pixpixpix/0.2.0/libexec/pixpixpix",
            false
        ),
        "Homebrew"
    );
    assert_eq!(
        method("/opt/homebrew/Cellar/pixpixpix/0.2.0/bin/pixpixpix", false),
        "Homebrew"
    );
    assert_eq!(
        method("/home/me/pixpixpix/pixpixpix", false),
        "release archive or own build"
    );
}
