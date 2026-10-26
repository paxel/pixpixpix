//! The external opener: hands a file to another program and lets it run on its
//! own. pixpixpix never waits for it; a thread reaps it and reports failure.

use std::path::Path;
use std::process::{Command, Stdio};

/// The command that opens `path` in `app`, or in the system default app
/// without one. On Linux `app` is a command line, split at whitespace, with the
/// file added last; on macOS it is an application name for `open -a`.
pub fn command(path: &Path, app: Option<&str>) -> Command {
    let mut command;
    if cfg!(target_os = "macos") {
        command = Command::new("open");
        if let Some(app) = app {
            command.arg("-a").arg(app.trim());
        }
    } else {
        let mut words = app.unwrap_or_default().split_whitespace();
        command = Command::new(words.next().unwrap_or("xdg-open"));
        command.args(words);
    }
    command.arg(path);
    command
}

/// Starts `command` detached from pixpixpix's input and output. A program
/// that cannot be started is an error right away; once it ends, `exited` gets
/// whether it reported success. The program keeps running when pixpixpix quits.
pub fn launch(
    mut command: Command,
    exited: impl FnOnce(Result<(), String>) + Send + 'static,
) -> Result<(), String> {
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| format!("Cannot start {program}: {e}"))?;
    std::thread::spawn(move || {
        let result = match child.wait() {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(format!("{program} failed: {status}")),
            Err(e) => Err(format!("{program} failed: {e}")),
        };
        exited(result);
    });
    Ok(())
}
