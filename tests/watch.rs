//! Seam 3: watching a real folder.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use pixpixpix::{folder, watch};

type Listing = Result<Vec<PathBuf>, String>;

/// Watches `dir`; the listings arrive on the returned receiver.
fn watching(dir: &Path) -> Result<(notify::RecommendedWatcher, Receiver<Listing>), String> {
    let (send, listings) = channel();
    let watcher = watch::watch(dir, move |listing| {
        let _ = send.send(listing);
    })?;
    Ok((watcher, listings))
}

/// The file names of the next listing, if one arrives soon.
fn next_listing(listings: &Receiver<Listing>) -> Option<Vec<String>> {
    let files = listings.recv_timeout(Duration::from_secs(5)).ok()?.ok()?;
    Some(files.iter().map(|f| folder::file_name(f)).collect())
}

#[test]
fn an_added_file_is_listed() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.jpg"), b"x").unwrap();
    let (_watcher, listings) = watching(dir.path()).unwrap();
    fs::write(dir.path().join("b.jpg"), b"x").unwrap();
    assert_eq!(
        next_listing(&listings),
        Some(["a.jpg", "b.jpg"].map(String::from).to_vec())
    );
}

#[test]
fn a_removed_file_is_no_longer_listed() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.jpg"), b"x").unwrap();
    fs::write(dir.path().join("b.jpg"), b"x").unwrap();
    let (_watcher, listings) = watching(dir.path()).unwrap();
    fs::remove_file(dir.path().join("a.jpg")).unwrap();
    assert_eq!(
        next_listing(&listings),
        Some(["b.jpg"].map(String::from).to_vec())
    );
}

#[test]
fn a_renamed_file_is_listed_under_its_new_name() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.jpg"), b"x").unwrap();
    fs::write(dir.path().join("b.jpg"), b"x").unwrap();
    let (_watcher, listings) = watching(dir.path()).unwrap();
    fs::rename(dir.path().join("a.jpg"), dir.path().join("c.jpg")).unwrap();
    assert_eq!(
        next_listing(&listings),
        Some(["b.jpg", "c.jpg"].map(String::from).to_vec())
    );
}

#[test]
fn a_burst_of_changes_gives_one_listing() {
    let dir = tempfile::tempdir().unwrap();
    let (_watcher, listings) = watching(dir.path()).unwrap();
    for name in ["a.jpg", "b.jpg", "c.jpg"] {
        fs::write(dir.path().join(name), b"x").unwrap();
    }
    assert_eq!(
        next_listing(&listings),
        Some(["a.jpg", "b.jpg", "c.jpg"].map(String::from).to_vec())
    );
    assert!(listings.recv_timeout(Duration::from_millis(600)).is_err());
}

#[test]
fn reading_a_file_is_no_change() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.jpg"), b"x").unwrap();
    let (_watcher, listings) = watching(dir.path()).unwrap();
    fs::read(dir.path().join("a.jpg")).unwrap();
    assert!(listings.recv_timeout(Duration::from_millis(600)).is_err());
}

#[test]
fn dropping_the_watcher_stops_the_listings() {
    let dir = tempfile::tempdir().unwrap();
    let (watcher, listings) = watching(dir.path()).unwrap();
    drop(watcher);
    fs::write(dir.path().join("a.jpg"), b"x").unwrap();
    assert!(listings.recv_timeout(Duration::from_millis(600)).is_err());
}
