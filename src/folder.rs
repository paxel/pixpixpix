//! Folder contents as the viewer sees them: which files count as media, and the
//! natural name order every list in the app uses.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{self, AtomicBool};

/// Extensions of the image formats pixpixpix opens, lower case.
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "png", "gif", "webp", "bmp", "tif", "tiff", "mpo",
];

/// True when the file name has an extension of a supported image format.
pub fn is_image(path: &Path) -> bool {
    has_extension(path, IMAGE_EXTENSIONS) || is_heif(path)
}

/// Extensions of HEIF and AVIF, the formats libheif decodes, lower case.
pub const HEIF_EXTENSIONS: &[&str] = &["heic", "heif", "hif", "avif"];

/// True when the file name has an extension of HEIF or AVIF.
pub fn is_heif(path: &Path) -> bool {
    has_extension(path, HEIF_EXTENSIONS)
}

/// Extensions of the video formats, lower case.
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "m4v", "mkv", "webm", "avi", "mts", "m2ts", "3gp", "wmv", "flv", "mpg",
];

/// True when the file name has an extension of a video format.
pub fn is_video(path: &Path) -> bool {
    has_extension(path, VIDEO_EXTENSIONS)
}

/// Extensions of the photos of an Apple Live Photo, lower case.
const LIVE_PHOTO_EXTENSIONS: &[&str] = &["heic", "heif", "jpg", "jpeg"];

/// Extensions of JPEG, lower case.
const JPEG_EXTENSIONS: &[&str] = &["jpg", "jpeg", "jpe"];

/// True when the file name has an extension of JPEG, the format of Google
/// and Samsung Motion Photos.
pub fn is_jpeg(path: &Path) -> bool {
    has_extension(path, JPEG_EXTENSIONS)
}

/// The Apple Live Photos among `files`: each photo (HEIC/HEIF or JPEG) with
/// the MOV of the same base name, ignoring case, in the same folder, as photo
/// to MOV. A base name pairs once, its first photo with its first MOV in the
/// order given.
pub fn live_photos(files: &[PathBuf]) -> BTreeMap<PathBuf, PathBuf> {
    let key = |path: &Path| {
        let stem = path.file_stem()?.to_string_lossy().to_lowercase();
        Some((path.parent()?.to_path_buf(), stem))
    };
    let mut movs = BTreeMap::new();
    for file in files.iter().filter(|f| has_extension(f, &["mov"])) {
        if let Some(key) = key(file) {
            movs.entry(key).or_insert(file);
        }
    }
    let mut pairs = BTreeMap::new();
    for photo in files
        .iter()
        .filter(|f| has_extension(f, LIVE_PHOTO_EXTENSIONS))
    {
        if let Some(mov) = key(photo).and_then(|k| movs.remove(&k)) {
            pairs.insert(photo.clone(), mov.clone());
        }
    }
    pairs
}

/// `files` without the MOVs of their Live Photos.
pub fn without_live_videos(files: Vec<PathBuf>) -> Vec<PathBuf> {
    let live = live_photos(&files);
    files
        .into_iter()
        .filter(|f| !live.values().any(|mov| mov == f))
        .collect()
}

/// True when the file is something navigation shows.
pub fn is_media(path: &Path) -> bool {
    is_image(path) || is_video(path)
}

fn has_extension(path: &Path, list: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| list.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// Natural order: digit runs compare by value, so `img2` sorts before `img10`.
/// Letters compare case-insensitively; ties fall back to the plain byte order so
/// the order is total.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut x = a.chars().peekable();
    let mut y = b.chars().peekable();
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let n = take_digits(&mut x);
                let m = take_digits(&mut y);
                let order = compare_numbers(&n, &m);
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(c), Some(d)) => {
                let order = c.to_lowercase().cmp(d.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                x.next();
                y.next();
            }
        }
    }
}

fn take_digits(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut out = String::new();
    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
        out.push(c);
        it.next();
    }
    out
}

/// Compares two digit strings by value without parsing, so any length works.
fn compare_numbers(n: &str, m: &str) -> Ordering {
    let n = n.trim_start_matches('0');
    let m = m.trim_start_matches('0');
    n.len().cmp(&m.len()).then_with(|| n.cmp(m))
}

/// Sorts paths by file name in natural order.
pub fn sort_natural(paths: &mut [PathBuf]) {
    paths.sort_by(|a, b| natural_cmp(&file_name(a), &file_name(b)));
}

/// The file name as text, lossy for names that are not UTF-8.
pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// True when the name starts with a dot: a hidden file or folder.
pub fn is_dot(path: &Path) -> bool {
    file_name(path).starts_with('.')
}

/// The media files directly in `dir`, in natural order. Dot files are skipped
/// unless `dots` is set.
pub fn list_media(dir: &Path, dots: bool) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if (!dots && is_dot(&path)) || !is_media(&path) {
            continue;
        }
        if entry.file_type()?.is_dir() {
            continue;
        }
        files.push(path);
    }
    sort_natural(&mut files);
    Ok(files)
}

/// The subfolders directly in `dir`, in natural order. Dot folders are skipped
/// unless `dots` is set; symlinks to folders count as folders.
pub fn list_dirs(dir: &Path, dots: bool) -> io::Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if (dots || !is_dot(&path)) && path.is_dir() {
            dirs.push(path);
        }
    }
    sort_natural(&mut dirs);
    Ok(dirs)
}

/// The first media file of `dir` in natural order, if any.
pub fn first_media(dir: &Path) -> io::Result<Option<PathBuf>> {
    Ok(list_media(dir, false)?.into_iter().next())
}

/// Walks `root` and the folders below it depth-first in natural order, a
/// folder's own files before its subfolders, calling `found` with every
/// folder that has media files and those files in natural order. Dot files
/// and dot folders are skipped unless `dots` is set, symlinked folders are not
/// followed and unreadable ones are passed over. False once `cancel` is set.
pub fn scan(
    root: &Path,
    dots: bool,
    cancel: &AtomicBool,
    mut found: impl FnMut(PathBuf, Vec<PathBuf>),
) -> bool {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if cancel.load(atomic::Ordering::Relaxed) {
            return false;
        }
        if let Ok(files) = list_media(&dir, dots)
            && !files.is_empty()
        {
            found(dir.clone(), files);
        }
        stack.extend(real_subfolders(&dir, dots).into_iter().rev());
    }
    true
}

/// The subfolders of `dir` that are no symlinks, in natural order; none when
/// it cannot be read.
fn real_subfolders(dir: &Path, dots: bool) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .filter(|p| dots || !is_dot(p))
        .collect();
    sort_natural(&mut dirs);
    dirs
}
