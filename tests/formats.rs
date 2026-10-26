//! Seam 3: decoding and folder reading on real fixture files.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use pixpixpix::decode::{self, DecodedImage};
use pixpixpix::{folder, startup};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Decodes `path` with a token that is never set.
fn decode_file(path: &Path) -> Result<DecodedImage, String> {
    decode::decode_file(path, &AtomicBool::new(false)).unwrap_or_else(|| Err("cancelled".into()))
}

fn pixel(image: &DecodedImage, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * image.width + x) * 4) as usize;
    [
        image.rgba[i],
        image.rgba[i + 1],
        image.rgba[i + 2],
        image.rgba[i + 3],
    ]
}

fn is_red(p: [u8; 4]) -> bool {
    p[0] > 200 && p[1] < 60 && p[2] < 60
}

fn is_blue(p: [u8; 4]) -> bool {
    p[0] < 60 && p[1] < 60 && p[2] > 200
}

#[test]
fn every_supported_format_decodes_to_red_left_blue_right() {
    for name in [
        "sample.png",
        "sample.jpg",
        "sample.gif",
        "sample.webp",
        "sample.bmp",
        "sample.tiff",
    ] {
        let image = decode_file(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!((image.width, image.height), (4, 2), "{name}");
        assert!(
            is_red(pixel(&image, 0, 0)),
            "{name}: {:?}",
            pixel(&image, 0, 0)
        );
        assert!(
            is_blue(pixel(&image, 3, 1)),
            "{name}: {:?}",
            pixel(&image, 3, 1)
        );
    }
}

#[test]
fn exif_orientation_is_applied() {
    // Orientation 6 means "rotate 90° clockwise to display": the red left half
    // ends up on top.
    let image = decode_file(&fixture("orientation6.jpg")).unwrap();
    assert_eq!((image.width, image.height), (2, 4));
    assert!(is_red(pixel(&image, 0, 0)));
    assert!(is_blue(pixel(&image, 1, 3)));
}

#[test]
fn truncated_file_reports_the_decoder_error() {
    let error = decode_file(&fixture("broken.jpg")).unwrap_err();
    assert!(!error.is_empty());
}

#[test]
fn cancelled_decode_reports_nothing() {
    let cancel = AtomicBool::new(true);
    assert_eq!(decode::decode_file(&fixture("sample.jpg"), &cancel), None);
}

#[test]
fn file_larger_than_one_read_chunk_decodes() {
    // A 400x400 PNG of noise is well over the 256 KiB read chunk.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.png");
    let noise = image::RgbaImage::from_fn(400, 400, |x, y| {
        let v = x
            .wrapping_mul(2_654_435_761)
            .wrapping_add(y.wrapping_mul(40_503));
        image::Rgba(v.to_le_bytes())
    });
    noise.save(&path).unwrap();
    assert!(std::fs::metadata(&path).unwrap().len() > 256 * 1024);
    let image = decode_file(&path).unwrap();
    assert_eq!((image.width, image.height), (400, 400));
    assert_eq!(pixel(&image, 7, 9), noise.get_pixel(7, 9).0);
}

#[test]
fn folder_lists_media_in_natural_order_without_dot_files() {
    let dir = tempfile::tempdir().unwrap();
    for name in [
        "img10.jpg",
        "img2.JPG",
        "IMG1.png",
        ".hidden.jpg",
        "notes.txt",
    ] {
        std::fs::write(dir.path().join(name), b"x").unwrap();
    }
    std::fs::create_dir(dir.path().join("sub.jpg")).unwrap();
    let names: Vec<String> = folder::list_media(dir.path())
        .unwrap()
        .iter()
        .map(|p| folder::file_name(p))
        .collect();
    assert_eq!(names, ["IMG1.png", "img2.JPG", "img10.jpg"]);
}

#[test]
fn start_with_a_file_uses_it_without_reading_the_folder() {
    let file = fixture("sample.png");
    assert_eq!(startup::resolve(Some(&file), Path::new("/")), Ok(file));
}

#[test]
fn start_with_a_folder_or_nothing_uses_its_first_image() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["b10.png", "b9.png"] {
        std::fs::write(dir.path().join(name), b"x").unwrap();
    }
    let first = dir.path().join("b9.png");
    assert_eq!(
        startup::resolve(Some(dir.path()), Path::new("/")),
        Ok(first.clone())
    );
    assert_eq!(startup::resolve(None, dir.path()), Ok(first.clone()));
    assert_eq!(
        startup::resolve(Some(Path::new(".")), dir.path()),
        Ok(dir.path().join(".").join("b9.png"))
    );
}

#[test]
fn start_with_an_empty_folder_explains_why() {
    let dir = tempfile::tempdir().unwrap();
    let message = startup::resolve(Some(dir.path()), Path::new("/")).unwrap_err();
    assert!(message.starts_with("No images in"), "{message}");
}
