//! The trash: moves files to the system trash, the freedesktop trash on Linux
//! and the Trash on macOS, from where they can be restored.

use std::path::Path;

use crate::folder;

/// Moves `path` to the system trash; the error names the file and why.
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
