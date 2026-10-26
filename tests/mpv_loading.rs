//! Seam 3: libmpv is loaded only once a video is decoded, and a missing one
//! is a clear error. A test binary of its own, so no other test loads it
//! first.

#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use pixpixpix::{decode, mpv};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Whether a libmpv is mapped into this process.
fn libmpv_loaded() -> std::io::Result<bool> {
    Ok(std::fs::read_to_string("/proc/self/maps")?.contains("libmpv.so"))
}

#[test]
fn libmpv_is_loaded_on_the_first_video_only() {
    let decode = |path: &Path| decode::decode_file(path, &AtomicBool::new(false)).unwrap();
    assert!(!libmpv_loaded().unwrap());
    decode(&fixture("sample.jpg")).unwrap();
    decode(&fixture("sample.png")).unwrap();
    assert!(!libmpv_loaded().unwrap());
    let dir = tempfile::tempdir().unwrap();
    let video = dir.path().join("clip.mp4");
    std::fs::write(&video, b"not a video").unwrap();
    match decode(&video) {
        Ok(image) => panic!("garbage decoded as {image:?}"),
        Err(error) if libmpv_loaded().unwrap() => assert!(!error.is_empty()),
        Err(error) => assert!(
            error.starts_with("Videos need libmpv, which was not found: "),
            "{error}"
        ),
    }
}

#[test]
fn a_missing_libmpv_says_so_with_the_reason() {
    let Err(error) = mpv::load(&["/nonexistent/libmpv.so.2"]) else {
        panic!("loaded a library that does not exist");
    };
    assert!(
        error.starts_with("Videos need libmpv, which was not found: /nonexistent/libmpv.so.2"),
        "{error}"
    );
}
