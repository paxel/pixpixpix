//! The trash: moves files to the system trash, the freedesktop trash on Linux
//! and the Trash on macOS, and puts them back. On Linux a file on another
//! drive goes into that drive's own trash folder, from where it is put back
//! the same way. macOS offers no way to put a file back, so pixpixpix keeps
//! where it put each file for as long as it runs.

use std::path::{Path, PathBuf};

use crate::core::BinItem;
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

/// Every media file in every trash: on Linux the home trash and the trash
/// folders of mounted drives, on macOS the Trash.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn list() -> Result<Vec<BinItem>, String> {
    let items = ::trash::os_limited::list().map_err(|e| format!("Cannot read the trash: {e}"))?;
    Ok(items
        .into_iter()
        .filter_map(|item| {
            // The id is `<trash>/info/<name>.trashinfo`, the file `<trash>/files/<name>`.
            let info = Path::new(&item.id);
            let trash = info.parent()?.parent()?;
            let name = info.file_name()?.to_str()?.strip_suffix(".trashinfo")?;
            let file = trash.join("files").join(name);
            (folder::is_media(&file) && file.is_file()).then(|| BinItem {
                original: Some(item.original_path()),
                file,
                deleted: item.time_deleted,
                id: item.id,
            })
        })
        .collect())
}

/// Puts `item` back where it came from; the error names the file and why.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn restore_item(item: &BinItem) -> Result<(), String> {
    use ::trash::os_limited;

    let name = item
        .original
        .as_deref()
        .map_or_else(String::new, folder::file_name);
    let items = os_limited::list().map_err(|e| format!("Cannot read the trash: {e}"))?;
    let found = items
        .into_iter()
        .find(|i| i.id == item.id)
        .ok_or_else(|| format!("Cannot restore {name}: it is not in the trash any more"))?;
    os_limited::restore_all([found]).map_err(|e| match e {
        ::trash::Error::RestoreCollision { .. } => {
            format!("Cannot restore {name}: a file of that name is back there")
        }
        other => format!("Cannot restore {name}: {other}"),
    })
}

/// Every media file in every trash: on Linux the home trash and the trash
/// folders of mounted drives, on macOS the Trash.
#[cfg(target_os = "macos")]
pub fn list() -> Result<Vec<BinItem>, String> {
    let trash = std::env::home_dir()
        .ok_or("Cannot find the Trash")?
        .join(".Trash");
    let entries = std::fs::read_dir(&trash).map_err(|e| format!("Cannot read the Trash: {e}"))?;
    let places = PLACES.lock().map(|p| p.clone()).unwrap_or_default();
    Ok(entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|file| folder::is_media(file) && file.is_file())
        .map(|file| {
            let deleted = std::fs::metadata(&file)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|d| i64::try_from(d.as_secs()).ok())
                .unwrap_or(0);
            BinItem {
                id: file.clone().into_os_string(),
                original: places
                    .iter()
                    .find(|(_, trashed)| **trashed == file)
                    .map(|(original, _)| original.clone()),
                file,
                deleted,
            }
        })
        .collect())
}

/// Puts `item` back where it came from; the error names the file and why.
#[cfg(target_os = "macos")]
pub fn restore_item(item: &BinItem) -> Result<(), String> {
    let name = folder::file_name(&item.file);
    let original = item.original.as_ref().ok_or_else(|| {
        format!(
            "Cannot restore {name}: macOS does not say where it came from; Finder's Put Back can"
        )
    })?;
    restore(std::slice::from_ref(original))
}

#[cfg(not(unix))]
pub fn list() -> Result<Vec<BinItem>, String> {
    Err("The trash cannot be listed on this system yet".to_owned())
}

#[cfg(not(unix))]
pub fn restore_item(_item: &BinItem) -> Result<(), String> {
    Err("Restoring from the trash is not supported on this system yet".to_owned())
}
