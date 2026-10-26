//! Folder contents as the viewer sees them: which files count as media, and the
//! natural name order every list in the app uses.

use std::cmp::Ordering;
use std::io;
use std::path::{Path, PathBuf};

/// Extensions of the image formats pixpixpix opens, lower case.
const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "png", "gif", "webp", "bmp", "tif", "tiff",
];

/// True when the file name has an extension of a supported image format.
pub fn is_image(path: &Path) -> bool {
    has_extension(path, IMAGE_EXTENSIONS)
}

/// True when the file is something navigation shows.
pub fn is_media(path: &Path) -> bool {
    is_image(path)
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

/// The media files directly in `dir`, in natural order. Dot files are skipped.
pub fn list_media(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if file_name(&path).starts_with('.') || !is_media(&path) {
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

/// The first media file of `dir` in natural order, if any.
pub fn first_media(dir: &Path) -> io::Result<Option<PathBuf>> {
    Ok(list_media(dir)?.into_iter().next())
}
