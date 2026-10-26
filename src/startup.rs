//! Deciding what to show first from the command line. Runs before the window
//! exists, so it touches the disk as little as possible: a file argument is used
//! as is, only a folder is read.

use std::path::{Path, PathBuf};

use crate::folder;

/// A folder given on the command line that has no file to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoFile {
    pub dir: PathBuf,
    /// Why there is nothing to show.
    pub message: String,
}

/// The file to open for `arg` (relative to `cwd`), or the folder and the message
/// to show when there is none. No argument means the current directory.
pub fn resolve(arg: Option<&Path>, cwd: &Path) -> Result<PathBuf, NoFile> {
    let target = match arg {
        Some(p) if p.is_absolute() => p.to_path_buf(),
        Some(p) => cwd.join(p),
        None => cwd.to_path_buf(),
    };
    if !target.is_dir() {
        return Ok(target);
    }
    let message = match folder::first_media(&target) {
        Ok(Some(file)) => return Ok(file),
        Ok(None) => format!("No images in {}", target.display()),
        Err(e) => format!("Cannot read {}: {e}", target.display()),
    };
    Err(NoFile {
        dir: target,
        message,
    })
}
