//! Deciding what to show first from the command line. Runs before the window
//! exists, so it touches the disk as little as possible: a file argument is used
//! as is, only a folder is read.

use std::path::{Path, PathBuf};

use crate::folder;

/// The file to open for `arg` (relative to `cwd`), or the message to show when
/// there is none. No argument means the current directory.
pub fn resolve(arg: Option<&Path>, cwd: &Path) -> Result<PathBuf, String> {
    let target = match arg {
        Some(p) if p.is_absolute() => p.to_path_buf(),
        Some(p) => cwd.join(p),
        None => cwd.to_path_buf(),
    };
    if !target.is_dir() {
        return Ok(target);
    }
    match folder::first_media(&target) {
        Ok(Some(file)) => Ok(file),
        Ok(None) => Err(format!("No images in {}", target.display())),
        Err(e) => Err(format!("Cannot read {}: {e}", target.display())),
    }
}
