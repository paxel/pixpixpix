//! Seam 3: libheif is loaded only once a HEIC/HEIF or AVIF file is decoded.
//! A test binary of its own, so no other test loads it first.

#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use pixpixpix::decode;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Whether a libheif is mapped into this process.
fn libheif_loaded() -> std::io::Result<bool> {
    Ok(std::fs::read_to_string("/proc/self/maps")?.contains("libheif.so"))
}

#[test]
fn libheif_is_loaded_on_the_first_heic_only() {
    let decode = |name| decode::decode_file(&fixture(name), &AtomicBool::new(false)).unwrap();
    assert!(!libheif_loaded().unwrap());
    decode("sample.jpg").unwrap();
    decode("sample.png").unwrap();
    assert!(!libheif_loaded().unwrap());
    match decode("sample.heic") {
        Ok(_) => assert!(libheif_loaded().unwrap()),
        Err(error) => assert!(
            error.starts_with("HEIC/HEIF and AVIF need libheif, which was not found"),
            "{error}"
        ),
    }
}
