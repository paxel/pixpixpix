//! Watching a folder: when files in it are added, removed, renamed or written,
//! the folder is listed again once the changes have settled.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use notify::event::ModifyKind;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::folder;

/// How long the folder must stay quiet before it is listed again, so a copy of
/// many files gives one listing, not hundreds.
const SETTLE: Duration = Duration::from_millis(200);

/// Watches `dir` and calls `on_change` with a fresh listing (as
/// [`folder::list_media`] gives it) after every burst of changes. Watching
/// stops when the returned watcher is dropped.
pub fn watch(
    dir: &Path,
    on_change: impl Fn(Result<Vec<PathBuf>, String>) + Send + 'static,
) -> Result<RecommendedWatcher, String> {
    let (changes, settled) = channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        // Reading files (our own decodes too) is no change of the folder.
        let relevant = event.map_or(true, |e| {
            !matches!(
                e.kind,
                EventKind::Access(_) | EventKind::Modify(ModifyKind::Metadata(_))
            )
        });
        if relevant {
            // The listing thread may be gone already; nothing to tell then.
            let _ = changes.send(());
        }
    })
    .map_err(|e| e.to_string())?;
    watcher
        .watch(dir, RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;
    let dir = dir.to_path_buf();
    std::thread::spawn(move || {
        // Ends when the watcher, and with it the sender, is dropped.
        while settled.recv().is_ok() {
            loop {
                match settled.recv_timeout(SETTLE) {
                    Ok(()) => {}
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            on_change(folder::list_media(&dir).map_err(|e| e.to_string()));
        }
    });
    Ok(watcher)
}
