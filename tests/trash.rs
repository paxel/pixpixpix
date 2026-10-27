//! Seam 3: moving real files to the trash, in a temporary folder.
//!
//! The trash goes where `XDG_DATA_HOME` points, so the test that really trashes
//! runs this test binary again with it set to a temporary folder, never
//! touching the user's trash.

use pixpixpix::trash;

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn a_file_goes_to_the_trash() {
    use std::path::Path;
    use std::process::Command;

    /// Set for the child run, to the folder holding the file and the trash.
    const CHILD_DIR: &str = "PIXPIXPIX_TRASH_TEST_DIR";

    if let Some(dir) = std::env::var_os(CHILD_DIR) {
        let dir = Path::new(&dir);
        let file = dir.join("a b.jpg");
        std::fs::write(&file, b"not really a jpeg").unwrap();
        trash::move_to_trash(&file).unwrap();
        assert!(!file.exists());
        let trashed = dir.join("data/Trash/files/a b.jpg");
        assert_eq!(std::fs::read(trashed).unwrap(), b"not really a jpeg");
        assert!(dir.join("data/Trash/info/a b.jpg.trashinfo").exists());
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "a_file_goes_to_the_trash", "--test-threads=1"])
        .env(CHILD_DIR, dir.path())
        .env("XDG_DATA_HOME", dir.path().join("data"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("1 passed"), "{stdout}");
}

#[test]
fn a_missing_file_is_an_error_naming_it() {
    let dir = tempfile::tempdir().unwrap();
    let error = trash::move_to_trash(&dir.path().join("gone.jpg")).unwrap_err();
    assert!(error.starts_with("Cannot trash gone.jpg: "), "{error}");
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn a_trashed_file_is_put_back_unless_its_place_is_taken() {
    use std::path::Path;
    use std::process::Command;

    /// Set for the child run, to the folder holding the file and the trash.
    const CHILD_DIR: &str = "PIXPIXPIX_RESTORE_TEST_DIR";

    if let Some(dir) = std::env::var_os(CHILD_DIR) {
        let dir = Path::new(&dir);
        let file = dir.join("a b.jpg");
        std::fs::write(&file, b"first").unwrap();
        trash::move_to_trash(&file).unwrap();
        trash::restore(std::slice::from_ref(&file)).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"first");
        // Trashed again, and the place taken meanwhile: it stays in the trash.
        trash::move_to_trash(&file).unwrap();
        std::fs::write(&file, b"second").unwrap();
        let error = trash::restore(std::slice::from_ref(&file)).unwrap_err();
        assert!(error.starts_with("Cannot restore a b.jpg: "), "{error}");
        assert_eq!(std::fs::read(&file).unwrap(), b"second");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "a_trashed_file_is_put_back_unless_its_place_is_taken",
            "--test-threads=1",
        ])
        .env(CHILD_DIR, dir.path())
        .env("XDG_DATA_HOME", dir.path().join("data"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("1 passed"), "{stdout}");
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn the_trash_lists_a_trashed_image_and_puts_that_one_back() {
    use std::path::Path;
    use std::process::Command;

    /// Set for the child run, to the folder holding the file and the trash.
    const CHILD_DIR: &str = "PIXPIXPIX_BIN_TEST_DIR";

    if let Some(dir) = std::env::var_os(CHILD_DIR) {
        let dir = Path::new(&dir);
        let file = dir.join("cat.jpg");
        std::fs::write(&file, b"cat").unwrap();
        std::fs::write(dir.join("notes.txt"), b"text").unwrap();
        trash::move_to_trash(&file).unwrap();
        trash::move_to_trash(&dir.join("notes.txt")).unwrap();
        let listed = trash::list().unwrap();
        // Only media files show.
        assert_eq!(listed.len(), 1, "{listed:?}");
        let item = &listed[0];
        assert_eq!(item.original.as_deref(), Some(file.as_path()));
        assert_eq!(std::fs::read(&item.file).unwrap(), b"cat");
        trash::restore_item(item).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"cat");
        assert!(trash::list().unwrap().is_empty());
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "the_trash_lists_a_trashed_image_and_puts_that_one_back",
            "--test-threads=1",
        ])
        .env(CHILD_DIR, dir.path())
        .env("XDG_DATA_HOME", dir.path().join("data"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("1 passed"), "{stdout}");
}
