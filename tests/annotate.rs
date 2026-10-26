//! Seam 3: drawing edits into the pixels and writing the edited image, on
//! real files.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use pixpixpix::annotate;
use pixpixpix::core::{Color, ImagePoint, Shape, Size};
use pixpixpix::decode::{self, DecodedImage};
use pixpixpix::rotate::Rotation;

const RED: [u8; 4] = [0xe5, 0x1c, 0x23, 0xff];
const WHITE: [u8; 4] = [0xff; 4];

fn white(width: u32, height: u32) -> DecodedImage {
    DecodedImage {
        width,
        height,
        rgba: Arc::from(vec![0xff; (width * height * 4) as usize]),
    }
}

/// A large red arrow across the middle of a 200x100 image, left to right.
fn arrow() -> Shape {
    Shape::Arrow {
        from: ImagePoint { x: 10.0, y: 50.0 },
        to: ImagePoint { x: 190.0, y: 50.0 },
        color: Color::Red,
        size: Size::Large,
    }
}

fn pixel(image: &DecodedImage, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * image.width + x) * 4) as usize;
    let mut pixel = [0; 4];
    pixel.copy_from_slice(&image.rgba[at..at + 4]);
    pixel
}

fn decoded(path: &Path) -> Option<DecodedImage> {
    decode::decode_file(path, &AtomicBool::new(false))?.ok()
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn render_draws_the_arrow_into_the_pixels() {
    let edited = annotate::render(&white(200, 100), &[arrow()]).unwrap();
    // On the shaft, and in the head just before the tip.
    assert_eq!(pixel(&edited, 50, 49), RED);
    assert_eq!(pixel(&edited, 50, 50), RED);
    assert_eq!(pixel(&edited, 180, 46), RED);
    // Away from it.
    assert_eq!(pixel(&edited, 50, 20), WHITE);
    assert_eq!(pixel(&edited, 5, 50), WHITE);
    assert_eq!(pixel(&edited, 195, 50), WHITE);
}

#[test]
fn render_without_shapes_keeps_the_pixels() {
    let image = white(4, 4);
    assert_eq!(annotate::render(&image, &[]).unwrap(), image);
}

#[test]
fn save_as_new_writes_the_edited_image_turned() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.png");
    let target = dir.path().join("a_edit.png");
    annotate::save(
        &source,
        &white(200, 100),
        Rotation::Right,
        &[arrow()],
        &target,
    )
    .unwrap();
    let written = decoded(&target).unwrap();
    assert_eq!((written.width, written.height), (100, 200));
    // The arrow runs top to bottom now, along x = 50.
    assert_eq!(pixel(&written, 50, 50), RED);
    assert_eq!(pixel(&written, 20, 50), WHITE);
}

#[test]
fn save_as_new_writes_jpeg_from_rgba() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a_edit.jpg");
    annotate::save(
        &dir.path().join("a.jpg"),
        &white(200, 100),
        Rotation::None,
        &[arrow()],
        &target,
    )
    .unwrap();
    let written = decoded(&target).unwrap();
    assert_eq!((written.width, written.height), (200, 100));
    let [r, g, b, _] = pixel(&written, 60, 50);
    assert!(r > 0xc0 && g < 0x50 && b < 0x50, "{r} {g} {b}");
}

#[test]
fn save_as_new_never_replaces_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a_edit.png");
    fs::write(&target, b"mine").unwrap();
    let error = annotate::save(
        &dir.path().join("a.png"),
        &white(200, 100),
        Rotation::None,
        &[arrow()],
        &target,
    )
    .unwrap_err();
    assert!(error.contains("a_edit.png"), "{error}");
    assert_eq!(fs::read(&target).unwrap(), b"mine");
}

#[test]
fn a_name_without_a_writable_format_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a_edit.heic");
    let result = annotate::save(
        &dir.path().join("a.png"),
        &white(200, 100),
        Rotation::None,
        &[arrow()],
        &target,
    );
    assert!(result.is_err());
    assert!(!target.exists());
}

#[test]
fn overwrite_replaces_the_file_with_the_edited_image() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.png");
    fs::copy(fixture("sample.png"), &path).unwrap();
    let image = decoded(&path).unwrap();
    let shape = Shape::Arrow {
        from: ImagePoint { x: 0.0, y: 0.0 },
        to: ImagePoint {
            x: image.width as f32,
            y: image.height as f32,
        },
        color: Color::Black,
        size: Size::Large,
    };
    annotate::save(&path, &image, Rotation::None, &[shape], &path).unwrap();
    let written = decoded(&path).unwrap();
    assert_eq!((written.width, written.height), (image.width, image.height));
    assert_eq!(written, annotate::render(&image, &[shape]).unwrap());
    // No temporary file is left behind.
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn overwrite_refuses_files_that_would_lose_something() {
    for name in ["pages.tiff", "sample.heic", "stereo.mpo"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        fs::copy(fixture(name), &path).unwrap();
        let before = fs::read(&path).unwrap();
        let result = annotate::save(&path, &white(200, 100), Rotation::None, &[arrow()], &path);
        assert!(result.is_err(), "{name}");
        assert_eq!(fs::read(&path).unwrap(), before, "{name}");
    }
}
