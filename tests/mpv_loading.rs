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

#[test]
fn the_bundled_libmpv_is_found_beside_the_app_executable_through_a_command_link() {
    let dir = tempfile::tempdir().unwrap();
    let macos = dir.path().join("pixpixpix.app/Contents/MacOS");
    std::fs::create_dir_all(&macos).unwrap();
    let exe = macos.join("pix3");
    std::fs::write(&exe, b"").unwrap();
    // As the cask links the commands into Homebrew's bin.
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    std::os::unix::fs::symlink(&exe, bin.join("pix3")).unwrap();
    let frameworks = std::fs::canonicalize(dir.path())
        .unwrap()
        .join("pixpixpix.app/Contents/Frameworks/libmpv.2.dylib");
    assert_eq!(mpv::bundled(&bin.join("pix3")), Some(frameworks.clone()));
    assert_eq!(mpv::bundled(&exe), Some(frameworks));
    assert_eq!(mpv::bundled(&dir.path().join("missing")), None);
}
