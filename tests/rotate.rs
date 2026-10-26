//! Seam 3: writing a rotation into real files, in a temporary folder.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use image::codecs::gif::GifEncoder;
use image::codecs::png::PngEncoder;
use image::metadata::Orientation;
use image::{
    DynamicImage, Frame, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, Rgba, RgbaImage,
};
use pixpixpix::decode::{self, DecodedImage};
use pixpixpix::rotate::{self, Rotation};
use pixpixpix::turbojpeg::{self, Transform};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// A copy of the fixture `name` in a fresh temporary folder.
fn copy(name: &str) -> std::io::Result<(tempfile::TempDir, PathBuf)> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join(name);
    std::fs::copy(fixture(name), &path)?;
    Ok((dir, path))
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

fn format(path: &Path) -> Option<ImageFormat> {
    let bytes = std::fs::read(path).ok()?;
    image::guess_format(&bytes).ok()
}

/// The names in `dir`.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

const REENCODED: [&str; 5] = [
    "sample.png",
    "sample.gif",
    "sample.webp",
    "sample.bmp",
    "sample.tiff",
];

#[test]
fn other_formats_are_reencoded_turned_right_in_their_own_format() {
    for name in REENCODED {
        let (dir, path) = copy(name).unwrap();
        let before = format(&path);
        assert!(before.is_some(), "{name}");
        rotate::write(&path, Rotation::Right).unwrap_or_else(|e| panic!("{name}: {e}"));
        let image = decode_file(&path).unwrap();
        // 4 × 2, red left and blue right, turned clockwise: red on top.
        assert_eq!((image.width, image.height), (2, 4), "{name}");
        assert!(is_red(pixel(&image, 1, 0)), "{name}");
        assert!(is_blue(pixel(&image, 0, 3)), "{name}");
        assert_eq!(format(&path), before, "{name}");
        assert_eq!(names(dir.path()), [name], "{name}");
    }
}

#[test]
fn left_and_half_turns_are_reencoded_too() {
    let (_dir, path) = copy("sample.png").unwrap();
    rotate::write(&path, Rotation::Left).unwrap();
    let image = decode_file(&path).unwrap();
    assert_eq!((image.width, image.height), (2, 4));
    assert!(is_red(pixel(&image, 0, 3)));
    assert!(is_blue(pixel(&image, 1, 0)));

    let (_dir, path) = copy("sample.png").unwrap();
    rotate::write(&path, Rotation::Half).unwrap();
    let image = decode_file(&path).unwrap();
    assert_eq!((image.width, image.height), (4, 2));
    assert!(is_red(pixel(&image, 3, 1)));
    assert!(is_blue(pixel(&image, 0, 0)));
}

/// TIFF-structured EXIF holding only orientation `value`.
fn exif_orientation(value: u8) -> Vec<u8> {
    let mut exif = b"MM\0\x2a\0\0\0\x08\0\x01".to_vec();
    exif.extend_from_slice(&[0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, value, 0, 0]);
    exif.extend_from_slice(&[0, 0, 0, 0]);
    exif
}

#[test]
fn the_exif_orientation_is_baked_into_reencoded_pixels() {
    // Stored 4 × 2 red left, blue right; orientation 6 shows it turned
    // clockwise, red on top.
    let stored = decode_file(&fixture("sample.png")).unwrap();
    let rgba = RgbaImage::from_raw(4, 2, stored.rgba.to_vec()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("turned.png");
    let mut bytes = Vec::new();
    let mut encoder = PngEncoder::new(&mut bytes);
    encoder.set_exif_metadata(exif_orientation(6)).unwrap();
    encoder
        .write_image(rgba.as_raw(), 4, 2, image::ExtendedColorType::Rgba8)
        .unwrap();
    std::fs::write(&path, bytes).unwrap();
    let shown = decode_file(&path).unwrap();
    assert_eq!((shown.width, shown.height), (2, 4));

    // A further clockwise turn of what is shown: blue on the left.
    rotate::write(&path, Rotation::Right).unwrap();
    let mut decoder = ImageReader::open(&path).unwrap().into_decoder().unwrap();
    assert_eq!(decoder.orientation().unwrap(), Orientation::NoTransforms);
    let image = decode_file(&path).unwrap();
    assert_eq!((image.width, image.height), (4, 2));
    assert!(is_blue(pixel(&image, 0, 0)));
    assert!(is_red(pixel(&image, 3, 1)));
}

#[test]
fn a_file_that_cannot_be_turned_stays_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.png");
    std::fs::write(&path, b"\x89PNG\r\n\x1a\nnot really").unwrap();
    let error = rotate::write(&path, Rotation::Right).unwrap_err();
    assert!(error.starts_with("Cannot rotate broken.png: "), "{error}");
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"\x89PNG\r\n\x1a\nnot really"
    );
    assert_eq!(names(dir.path()), ["broken.png"]);
}

#[test]
fn animated_images_are_refused_rather_than_flattened() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("moving.gif");
    let mut bytes = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut bytes);
        for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
            let frame = Frame::new(RgbaImage::from_pixel(4, 2, Rgba(color)));
            encoder.encode_frame(frame).unwrap();
        }
    }
    std::fs::write(&path, &bytes).unwrap();
    let error = rotate::write(&path, Rotation::Right).unwrap_err();
    assert!(error.contains("animated"), "{error}");
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn a_missing_libturbojpeg_is_an_error_naming_it() {
    let error = turbojpeg::load(&["libpixpixpix-no-such-library.so"])
        .err()
        .unwrap();
    assert!(
        error.starts_with("Lossless JPEG rotation needs libturbojpeg"),
        "{error}"
    );
}

/// What a lossless transform does to the stored pixels, done with `image`.
fn apply(image: &DynamicImage, transform: Transform) -> DynamicImage {
    match transform {
        Transform::None => image.clone(),
        Transform::FlipHorizontal => image.fliph(),
        Transform::FlipVertical => image.flipv(),
        Transform::Transpose => image.rotate90().fliph(),
        Transform::Transverse => image.rotate270().fliph(),
        Transform::Rotate90 => image.rotate90(),
        Transform::Rotate180 => image.rotate180(),
        Transform::Rotate270 => image.rotate270(),
    }
}

#[test]
fn the_jpeg_transform_turns_stored_pixels_into_the_shown_ones_turned() {
    // Every pixel different, so any wrong transform shows.
    let stored = DynamicImage::ImageRgba8(RgbaImage::from_fn(3, 2, |x, y| {
        Rgba([x as u8, y as u8, 0, 255])
    }));
    for exif in 1..=8 {
        let orientation = Orientation::from_exif(exif).unwrap();
        for rotation in [
            Rotation::None,
            Rotation::Right,
            Rotation::Half,
            Rotation::Left,
        ] {
            let mut expected = stored.clone();
            expected.apply_orientation(orientation);
            let expected = match rotation {
                Rotation::None => expected,
                Rotation::Right => expected.rotate90(),
                Rotation::Half => expected.rotate180(),
                Rotation::Left => expected.rotate270(),
            };
            let transform = rotate::jpeg_transform(orientation, rotation);
            assert_eq!(
                apply(&stored, transform),
                expected,
                "orientation {exif}, {rotation:?}: {transform:?}"
            );
        }
    }
}

#[test]
fn resetting_the_orientation_keeps_everything_else() {
    let original = std::fs::read(fixture("orientation6.jpg")).unwrap();
    let mut jpeg = original.clone();
    rotate::reset_orientation(&mut jpeg);
    let mut decoder = ImageReader::new(Cursor::new(&jpeg))
        .with_guessed_format()
        .unwrap()
        .into_decoder()
        .unwrap();
    assert_eq!(decoder.orientation().unwrap(), Orientation::NoTransforms);
    let changed = original.iter().zip(&jpeg).filter(|(a, b)| a != b).count();
    assert_eq!(changed, 1);
    assert_eq!(original.len(), jpeg.len());
}

/// 16 × 16, a multiple of every JPEG block size: red on the left half, blue
/// on the right.
fn aligned_jpeg(path: &Path) -> image::ImageResult<()> {
    let image = RgbaImage::from_fn(16, 16, |x, _| {
        if x < 8 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 255, 255])
        }
    });
    DynamicImage::ImageRgba8(image)
        .to_rgb8()
        .save_with_format(path, ImageFormat::Jpeg)
}

#[test]
fn jpeg_is_turned_losslessly_or_refused_without_libturbojpeg() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photo.jpg");
    aligned_jpeg(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    match turbojpeg::load(turbojpeg::LIBRARY_NAMES) {
        Ok(_) => {
            rotate::write(&path, Rotation::Right).unwrap();
            let image = decode_file(&path).unwrap();
            assert_eq!((image.width, image.height), (16, 16));
            assert!(is_red(pixel(&image, 8, 2)));
            assert!(is_blue(pixel(&image, 8, 13)));
            assert_eq!(format(&path), Some(ImageFormat::Jpeg));
        }
        Err(_) => {
            let error = rotate::write(&path, Rotation::Right).unwrap_err();
            assert!(
                error.starts_with(
                    "Cannot rotate photo.jpg: Lossless JPEG rotation needs libturbojpeg"
                ),
                "{error}"
            );
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
    }
    assert_eq!(names(dir.path()), ["photo.jpg"]);
}
