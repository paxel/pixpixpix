//! Seam 3: drawing edits into the pixels and writing the edited image, on
//! real files.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use pixpixpix::annotate;
use pixpixpix::core::{Color, ImagePoint, Region, Shape, Size};
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
    annotate::save(
        &path,
        &image,
        Rotation::None,
        std::slice::from_ref(&shape),
        &path,
    )
    .unwrap();
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

fn point(x: f32, y: f32) -> ImagePoint {
    ImagePoint { x, y }
}

/// A 200x100 image whose every pixel differs from its neighbours.
fn gradient() -> DecodedImage {
    let mut rgba = Vec::new();
    for y in 0..100u32 {
        for x in 0..200u32 {
            rgba.extend([x as u8, (y * 2) as u8, ((x + y) * 3 % 256) as u8, 0xff]);
        }
    }
    DecodedImage {
        width: 200,
        height: 100,
        rgba: Arc::from(rgba),
    }
}

fn is_white(pixel: [u8; 4]) -> bool {
    pixel == WHITE
}

/// Whether any pixel within `xs` × `ys` is not white.
fn marked(image: &DecodedImage, xs: std::ops::Range<u32>, ys: std::ops::Range<u32>) -> bool {
    ys.flat_map(|y| xs.clone().map(move |x| (x, y)))
        .any(|(x, y)| !is_white(pixel(image, x, y)))
}

#[test]
fn pixelate_fills_blocks_of_one_colour_that_hide_the_pixels() {
    let image = gradient();
    let shape = Shape::Pixelate {
        from: point(20.0, 10.0),
        to: point(80.0, 50.0),
        size: Size::Medium,
    };
    let block = Size::Medium.block(200, 100);
    assert!(block >= 4);
    let edited = annotate::render(&image, std::slice::from_ref(&shape)).unwrap();
    // Every block is one colour, which no longer shows the gradient.
    let blocks = shape.blocks(&image);
    assert!(blocks.len() > 4);
    for b in &blocks {
        let first = pixel(&edited, b.left, b.top);
        for y in b.top..b.top + b.height {
            for x in b.left..b.left + b.width {
                assert_eq!(pixel(&edited, x, y), first, "{x} {y}");
            }
        }
        assert_eq!(first[..3], b.rgb);
    }
    let changed = (10..50)
        .flat_map(|y| (20..80).map(move |x| (x, y)))
        .filter(|&(x, y)| pixel(&edited, x, y) != pixel(&image, x, y))
        .count();
    assert!(changed > 60 * 40 * 9 / 10, "{changed}");
    // Outside the box nothing changed.
    assert_eq!(pixel(&edited, 19, 30), pixel(&image, 19, 30));
    assert_eq!(pixel(&edited, 50, 50), pixel(&image, 50, 50));
}

#[test]
fn the_saved_pixelation_hides_the_area() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a_edit.png");
    let shape = Shape::Pixelate {
        from: point(0.0, 0.0),
        to: point(200.0, 100.0),
        size: Size::Large,
    };
    let image = gradient();
    annotate::save(
        &dir.path().join("a.png"),
        &image,
        Rotation::None,
        &[shape],
        &target,
    )
    .unwrap();
    let written = decoded(&target).unwrap();
    // Neighbours within a block are alike; in the original they differ.
    assert_eq!(pixel(&written, 0, 0), pixel(&written, 1, 1));
    assert_ne!(pixel(&image, 0, 0), pixel(&image, 1, 1));
}

#[test]
fn the_highlighter_lays_a_see_through_colour() {
    let edited = annotate::render(
        &white(200, 100),
        &[Shape::Highlighter {
            from: point(10.0, 10.0),
            to: point(100.0, 50.0),
            color: Color::Red,
        }],
    )
    .unwrap();
    let [r, g, b, a] = pixel(&edited, 50, 30);
    // Between white and the red of the palette.
    assert_eq!(a, 0xff);
    assert!(r > RED[0] && g > RED[1] && g < 0xff && b > RED[2] && b < 0xff);
    assert_eq!(pixel(&edited, 150, 30), WHITE);
}

#[test]
fn outlines_and_the_pen_draw_lines_only() {
    let shapes = [
        Shape::Rectangle {
            from: point(10.0, 10.0),
            to: point(90.0, 90.0),
            color: Color::Red,
            size: Size::Large,
        },
        Shape::Ellipse {
            from: point(110.0, 10.0),
            to: point(190.0, 90.0),
            color: Color::Red,
            size: Size::Large,
        },
        Shape::Pen {
            points: vec![point(10.0, 95.0), point(190.0, 95.0)],
            color: Color::Red,
            size: Size::Large,
        },
    ];
    let edited = annotate::render(&white(200, 100), &shapes).unwrap();
    // On the rectangle's sides, the ellipse's left and top, the pen line.
    for (x, y) in [
        (10, 50),
        (50, 10),
        (90, 50),
        (110, 50),
        (150, 10),
        (100, 95),
    ] {
        assert_eq!(pixel(&edited, x, y), RED, "{x} {y}");
    }
    // Inside the outlines and in the ellipse's box corner.
    for (x, y) in [(50, 50), (150, 50), (113, 13)] {
        assert_eq!(pixel(&edited, x, y), WHITE, "{x} {y}");
    }
}

#[test]
fn text_is_drawn_from_its_corner() {
    let text = |turn| Shape::Text {
        at: point(50.0, 60.0),
        text: "HH".to_owned(),
        color: Color::Black,
        size: Size::Large,
        turn,
    };
    let edited = annotate::render(&white(400, 200), &[text(Rotation::None)]).unwrap();
    // 24 pixels to the em: the letters right of and below the corner.
    assert!(marked(&edited, 50..80, 60..84));
    assert!(!marked(&edited, 0..400, 0..60));
    assert!(!marked(&edited, 0..50, 0..200));
    assert!(!marked(&edited, 90..400, 0..200));
    // Placed on the image turned right, it runs upwards from the corner.
    let edited = annotate::render(&white(400, 200), &[text(Rotation::Right)]).unwrap();
    assert!(marked(&edited, 50..74, 30..60));
    assert!(!marked(&edited, 0..50, 0..200));
    assert!(!marked(&edited, 0..400, 61..200));
}

#[test]
fn the_crop_is_applied_after_the_shapes_and_before_the_turn() {
    let crop = Shape::Crop(Region {
        left: 20.0,
        top: 10.0,
        right: 120.0,
        bottom: 60.0,
    });
    let edited = annotate::render(&white(200, 100), &[arrow(), crop.clone()]).unwrap();
    assert_eq!((edited.width, edited.height), (100, 50));
    // The arrow at y = 50 shows 40 pixels down.
    assert_eq!(pixel(&edited, 30, 40), RED);
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a_edit.png");
    annotate::save(
        &dir.path().join("a.png"),
        &white(200, 100),
        Rotation::Right,
        &[arrow(), crop],
        &target,
    )
    .unwrap();
    let written = decoded(&target).unwrap();
    assert_eq!((written.width, written.height), (50, 100));
}
