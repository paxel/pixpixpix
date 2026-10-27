//! Seam 3: the external opener, with real programs.

use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;

use pixpixpix::external;

fn args(command: &std::process::Command) -> Vec<String> {
    command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn without_an_app_the_system_default_opens_the_file() {
    let command = external::command(Path::new("/p/a b.jpg"), None);
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    assert_eq!(command.get_program(), program);
    assert_eq!(args(&command), ["/p/a b.jpg"]);
}

#[test]
fn an_app_gets_the_file_last() {
    let command = external::command(Path::new("/p/a.jpg"), Some(" vlc  --fullscreen "));
    if cfg!(target_os = "macos") {
        assert_eq!(command.get_program(), "open");
        assert_eq!(args(&command), ["-a", "vlc  --fullscreen", "/p/a.jpg"]);
    } else {
        assert_eq!(command.get_program(), "vlc");
        assert_eq!(args(&command), ["--fullscreen", "/p/a.jpg"]);
    }
}

#[cfg(unix)]
#[test]
fn the_program_runs_with_the_file_and_success_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    let script = dir.path().join("viewer");
    std::fs::write(
        &script,
        format!("printf '%s' \"$1\" > '{}'\n", out.display()),
    )
    .unwrap();
    let file = dir.path().join("a b.jpg");
    // `sh` reads the script instead of executing it: a just-written file can
    // still be open in a child another test is forking ("Text file busy").
    let mut command = std::process::Command::new("sh");
    command.arg(&script).arg(&file);
    let (sender, exited) = channel();
    external::launch(command, move |result| sender.send(result).unwrap()).unwrap();
    assert_eq!(exited.recv_timeout(Duration::from_secs(10)), Ok(Ok(())));
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        file.display().to_string()
    );
}

#[test]
fn a_program_that_fails_is_reported_with_its_status() {
    let (sender, exited) = channel();
    let command = std::process::Command::new("false");
    external::launch(command, move |result| sender.send(result).unwrap()).unwrap();
    let error = exited
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap_err();
    assert!(error.starts_with("false failed"), "{error}");
}

#[test]
fn a_missing_program_is_an_error_naming_it() {
    // `open -a` itself starts on macOS and only then fails with its status.
    let missing = if cfg!(target_os = "macos") {
        std::process::Command::new("pixpixpix-no-such-viewer")
    } else {
        external::command(Path::new("/p/a.jpg"), Some("pixpixpix-no-such-viewer"))
    };
    let error = external::launch(missing, |_| panic!("never started")).unwrap_err();
    assert!(
        error.starts_with("Cannot start pixpixpix-no-such-viewer:"),
        "{error}"
    );
}
