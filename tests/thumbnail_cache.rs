//! Seam 3: the freedesktop thumbnail cache on real files in a temporary
//! cache root.

use std::fs::File;
use std::io::BufReader;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime};

use pixpixpix::decode::DecodedImage;
use pixpixpix::thumbnails;

const RED: image::Rgba<u8> = image::Rgba([255, 0, 0, 255]);
const GREEN: image::Rgba<u8> = image::Rgba([0, 255, 0, 255]);

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// A `width`×`height` image of `color` at `path`, last modified at `mtime`
/// seconds.
fn image_file(
    path: &Path,
    width: u32,
    height: u32,
    color: image::Rgba<u8>,
    mtime: u64,
) -> Result<()> {
    image::RgbaImage::from_pixel(width, height, color).save(path)?;
    File::options()
        .write(true)
        .open(path)?
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(mtime))?;
    Ok(())
}

/// An 8×8 thumbnail PNG of `color` at `at`, carrying `chunks` as tEXt.
fn cache_file(at: &Path, color: image::Rgba<u8>, chunks: &[(&str, String)]) -> Result<()> {
    if let Some(dir) = at.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut encoder = png::Encoder::new(File::create(at)?, 8, 8);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    for (key, value) in chunks {
        encoder.add_text_chunk((*key).to_owned(), value.clone())?;
    }
    let pixels = image::RgbaImage::from_pixel(8, 8, color).into_raw();
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&pixels)?;
    writer.finish()?;
    Ok(())
}

/// The tEXt chunks of the PNG at `path`.
fn chunks(path: &Path) -> Result<Vec<(String, String)>> {
    let mut reader = png::Decoder::new(BufReader::new(File::open(path)?)).read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size().ok_or("too large")?];
    reader.next_frame(&mut pixels)?;
    reader.finish()?;
    Ok(reader
        .info()
        .uncompressed_latin1_text
        .iter()
        .map(|chunk| (chunk.keyword.clone(), chunk.text.clone()))
        .collect())
}

/// The thumbnail of `path` with a token that is never set.
fn thumbnail(root: &Path, path: &Path, max: u32) -> std::result::Result<DecodedImage, String> {
    thumbnails::thumbnail(Some(root), path, max, &AtomicBool::new(false))
        .unwrap_or_else(|| Err("cancelled".into()))
}

fn first_pixel(image: &DecodedImage) -> Option<[u8; 4]> {
    Some([
        *image.rgba.first()?,
        *image.rgba.get(1)?,
        *image.rgba.get(2)?,
        *image.rgba.get(3)?,
    ])
}

/// The cache file of `path` at 256 pixels in `root`.
fn large(root: &Path, path: &Path) -> PathBuf {
    thumbnails::cache_path(root, path, 256).unwrap_or_default()
}

fn uri(path: &Path) -> String {
    thumbnails::uri(path).unwrap_or_default()
}

fn mode(path: &Path) -> std::io::Result<u32> {
    Ok(std::fs::metadata(path)?.permissions().mode() & 0o777)
}

#[test]
fn the_cache_file_is_named_after_the_md5_of_the_uri() {
    let root = Path::new("/cache");
    let path = Path::new("/home/jens/photos/me.png");
    assert_eq!(uri(path), "file:///home/jens/photos/me.png");
    // The example of the spec.
    assert_eq!(
        thumbnails::cache_path(root, path, 128).unwrap(),
        root.join("normal/c6ee772d9e49320e97ec29a7eb5b1697.png")
    );
}

#[test]
fn the_uri_escapes_like_glib() {
    let path = Path::new("/home/jens/my photos/#1;é.jpg");
    assert_eq!(uri(path), "file:///home/jens/my%20photos/%231%3B%C3%A9.jpg");
    assert_eq!(
        thumbnails::cache_path(Path::new("/cache"), path, 128).unwrap(),
        Path::new("/cache/normal/238170899a66a05801b3c522a42e33d0.png")
    );
    let path = Path::new("/tmp/a&b (1)+!~$,='@:.jpg");
    assert_eq!(uri(path), "file:///tmp/a&b%20(1)+!~$,='@:.jpg");
    assert_eq!(
        thumbnails::cache_path(Path::new("/cache"), path, 128).unwrap(),
        Path::new("/cache/normal/f2daf92179e7a4cff10696c404e4cf10.png")
    );
}

#[test]
fn the_size_dir_is_the_smallest_that_holds_the_size() {
    let root = Path::new("/cache");
    let path = Path::new("/a.jpg");
    let dir = |max| {
        thumbnails::cache_path(root, path, max)
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    };
    assert_eq!(dir(96), root.join("normal"));
    assert_eq!(dir(128), root.join("normal"));
    assert_eq!(dir(256), root.join("large"));
    assert_eq!(dir(320), root.join("x-large"));
    assert_eq!(dir(1024), root.join("xx-large"));
}

#[test]
fn a_new_thumbnail_is_written_with_uri_mtime_and_size() {
    let files = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let root = cache.path().join("thumbnails");
    let path = files.path().join("wide.png");
    image_file(&path, 600, 300, RED, 1_600_000_000).unwrap();

    let shown = thumbnail(&root, &path, 256).unwrap();

    assert_eq!((shown.width, shown.height), (256, 128));
    let written = large(&root, &path);
    let saved = image::open(&written).unwrap();
    assert_eq!((saved.width(), saved.height()), (256, 128));
    let chunks = chunks(&written).unwrap();
    let size = std::fs::metadata(&path).unwrap().len();
    assert!(chunks.contains(&("Thumb::URI".to_owned(), uri(&path))));
    assert!(chunks.contains(&("Thumb::MTime".to_owned(), "1600000000".to_owned())));
    assert!(chunks.contains(&("Thumb::Size".to_owned(), size.to_string())));
}

#[test]
fn a_new_thumbnail_is_private_and_leaves_no_temporary_file() {
    let files = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let root = cache.path().join("thumbnails");
    let path = files.path().join("a.png");
    image_file(&path, 300, 300, RED, 1_600_000_000).unwrap();

    thumbnail(&root, &path, 256).unwrap();

    let written = large(&root, &path);
    assert_eq!(mode(&written).unwrap(), 0o600);
    assert_eq!(mode(&root).unwrap(), 0o700);
    assert_eq!(mode(&root.join("large")).unwrap(), 0o700);
    let names: Vec<_> = std::fs::read_dir(root.join("large"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(names, vec![written]);
}

#[test]
fn a_valid_cached_thumbnail_is_used_instead_of_decoding() {
    let files = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let path = files.path().join("a.png");
    image_file(&path, 300, 300, RED, 1_600_000_000).unwrap();
    let size = std::fs::metadata(&path).unwrap().len();
    cache_file(
        &large(cache.path(), &path),
        GREEN,
        &[
            ("Thumb::URI", uri(&path)),
            ("Thumb::MTime", "1600000000".to_owned()),
            ("Thumb::Size", size.to_string()),
        ],
    )
    .unwrap();

    let shown = thumbnail(cache.path(), &path, 256).unwrap();

    assert_eq!((shown.width, shown.height), (8, 8));
    assert_eq!(first_pixel(&shown).unwrap(), [0, 255, 0, 255]);
}

#[test]
fn a_cached_thumbnail_is_used_without_a_size_tag() {
    let files = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let path = files.path().join("a.png");
    image_file(&path, 300, 300, RED, 1_600_000_000).unwrap();
    cache_file(
        &large(cache.path(), &path),
        GREEN,
        &[
            ("Thumb::URI", uri(&path)),
            ("Thumb::MTime", "1600000000".to_owned()),
        ],
    )
    .unwrap();

    let shown = thumbnail(cache.path(), &path, 256).unwrap();

    assert_eq!(first_pixel(&shown).unwrap(), [0, 255, 0, 255]);
}

#[test]
fn an_outdated_thumbnail_is_regenerated() {
    let files = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let path = files.path().join("a.png");
    image_file(&path, 300, 300, RED, 1_600_000_000).unwrap();
    let cached = large(cache.path(), &path);
    cache_file(
        &cached,
        GREEN,
        &[
            ("Thumb::URI", uri(&path)),
            ("Thumb::MTime", "1500000000".to_owned()),
        ],
    )
    .unwrap();

    let shown = thumbnail(cache.path(), &path, 256).unwrap();

    assert_eq!(first_pixel(&shown).unwrap(), [255, 0, 0, 255]);
    assert!(
        chunks(&cached)
            .unwrap()
            .contains(&("Thumb::MTime".to_owned(), "1600000000".to_owned()))
    );
}

#[test]
fn a_thumbnail_of_a_rewritten_file_is_regenerated() {
    let files = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let path = files.path().join("a.png");
    image_file(&path, 300, 300, RED, 1_600_000_000).unwrap();
    cache_file(
        &large(cache.path(), &path),
        GREEN,
        &[
            ("Thumb::URI", uri(&path)),
            ("Thumb::MTime", "1600000000".to_owned()),
            ("Thumb::Size", "1".to_owned()),
        ],
    )
    .unwrap();

    let shown = thumbnail(cache.path(), &path, 256).unwrap();

    assert_eq!(first_pixel(&shown).unwrap(), [255, 0, 0, 255]);
}

#[test]
fn a_thumbnail_of_another_file_is_not_used() {
    let files = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let path = files.path().join("a.png");
    image_file(&path, 300, 300, RED, 1_600_000_000).unwrap();
    cache_file(
        &large(cache.path(), &path),
        GREEN,
        &[
            ("Thumb::URI", "file:///elsewhere.png".to_owned()),
            ("Thumb::MTime", "1600000000".to_owned()),
        ],
    )
    .unwrap();

    let shown = thumbnail(cache.path(), &path, 256).unwrap();

    assert_eq!(first_pixel(&shown).unwrap(), [255, 0, 0, 255]);
}

#[test]
fn a_cached_thumbnail_is_shrunk_to_the_size() {
    let files = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let path = files.path().join("a.png");
    image_file(&path, 600, 300, RED, 1_600_000_000).unwrap();

    thumbnail(cache.path(), &path, 256).unwrap();
    let shown = thumbnail(cache.path(), &path, 200).unwrap();

    assert_eq!((shown.width, shown.height), (200, 100));
}

#[test]
fn a_broken_file_writes_no_thumbnail() {
    let files = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let path = files.path().join("broken.jpg");
    std::fs::write(&path, b"not a jpeg").unwrap();

    let result = thumbnails::thumbnail(Some(cache.path()), &path, 256, &AtomicBool::new(false));

    assert!(matches!(result, Some(Err(_))));
    assert!(!large(cache.path(), &path).exists());
}

#[test]
fn a_file_inside_the_cache_gets_no_thumbnail_of_its_own() {
    let cache = tempfile::tempdir().unwrap();
    let path = cache.path().join("large/0123.png");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    image_file(&path, 300, 300, RED, 1_600_000_000).unwrap();

    let shown = thumbnail(cache.path(), &path, 256).unwrap();

    assert_eq!(first_pixel(&shown).unwrap(), [255, 0, 0, 255]);
    assert!(!large(cache.path(), &path).exists());
}

#[test]
fn without_a_cache_the_file_is_decoded() {
    let files = tempfile::tempdir().unwrap();
    let path = files.path().join("a.png");
    image_file(&path, 600, 300, RED, 1_600_000_000).unwrap();

    let shown = thumbnails::thumbnail(None, &path, 256, &AtomicBool::new(false))
        .unwrap()
        .unwrap();

    assert_eq!((shown.width, shown.height), (256, 128));
}

#[test]
fn a_cancelled_thumbnail_writes_nothing() {
    let files = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let path = files.path().join("a.png");
    image_file(&path, 300, 300, RED, 1_600_000_000).unwrap();

    let result = thumbnails::thumbnail(Some(cache.path()), &path, 256, &AtomicBool::new(true));

    assert!(result.is_none());
    assert!(!large(cache.path(), &path).exists());
}
