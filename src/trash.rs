//! The trash: moves files to the system trash, the freedesktop trash on Linux
//! and the Trash on macOS, and puts them back. On Linux a file on another
//! drive goes into that drive's own trash folder, from where it is put back
//! the same way. macOS offers no way to put a file back, so pixpixpix keeps
//! where it put each file for as long as it runs.

use std::path::{Path, PathBuf};

use crate::folder;

/// Moves `path` to the system trash; the error names the file and why.
#[cfg(target_os = "macos")]
pub fn move_to_trash(path: &Path) -> Result<(), String> {
    let trashed = crate::macos::trash(path)
        .map_err(|reason| format!("Cannot trash {}: {reason}", folder::file_name(path)))?;
    if let Ok(mut places) = PLACES.lock() {
        places.insert(path.to_path_buf(), trashed);
    }
    Ok(())
}

/// Where the files trashed on macOS went, by their original path.
#[cfg(target_os = "macos")]
static PLACES: std::sync::Mutex<std::collections::BTreeMap<PathBuf, PathBuf>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

/// Puts the files at `paths` back from the trash, each the one trashed last
/// from there; the error names a file and why.
#[cfg(target_os = "macos")]
pub fn restore(paths: &[PathBuf]) -> Result<(), String> {
    for path in paths {
        let cannot = |reason: &str| format!("Cannot restore {}: {reason}", folder::file_name(path));
        if path.exists() {
            return Err(cannot("a file of that name is back there"));
        }
        let trashed = PLACES
            .lock()
            .ok()
            .and_then(|mut places| places.remove(path))
            .ok_or_else(|| cannot("it is not in the trash any more"))?;
        std::fs::rename(&trashed, path).map_err(|e| cannot(&e.to_string()))?;
    }
    Ok(())
}

/// Puts the files at `paths` back from the trash, each the one trashed last
/// from there; the error names a file and why.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn restore(paths: &[PathBuf]) -> Result<(), String> {
    use ::trash::os_limited;

    let items = os_limited::list().map_err(|e| format!("Cannot read the trash: {e}"))?;
    let mut chosen = Vec::new();
    for path in paths {
        let item = items
            .iter()
            .filter(|item| item.original_path() == *path)
            .max_by_key(|item| item.time_deleted)
            .ok_or_else(|| {
                format!(
                    "Cannot restore {}: it is not in the trash any more",
                    folder::file_name(path)
                )
            })?;
        chosen.push(item.clone());
    }
    os_limited::restore_all(chosen).map_err(|e| match e {
        ::trash::Error::RestoreCollision { path, .. } => format!(
            "Cannot restore {}: a file of that name is back there",
            folder::file_name(&path)
        ),
        other => format!("Cannot restore: {other}"),
    })
}

#[cfg(not(unix))]
pub fn restore(_paths: &[PathBuf]) -> Result<(), String> {
    Err("Restoring from the trash is not supported on this system yet".to_owned())
}

/// Moves `path` to the system trash; the error names the file and why.
#[cfg(not(target_os = "macos"))]
pub fn move_to_trash(path: &Path) -> Result<(), String> {
    ::trash::delete(path).map_err(|e| {
        let reason = match e {
            ::trash::Error::Unknown { description } | ::trash::Error::Os { description, .. } => {
                description
            }
            #[cfg(all(
                unix,
                not(target_os = "macos"),
                not(target_os = "ios"),
                not(target_os = "android")
            ))]
            ::trash::Error::FileSystem { source, .. } => source.to_string(),
            ::trash::Error::CouldNotAccess { .. } | ::trash::Error::CanonicalizePath { .. } => {
                "it cannot be accessed".to_owned()
            }
            other => other.to_string(),
        };
        format!("Cannot trash {}: {reason}", folder::file_name(path))
    })
}
