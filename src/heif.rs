//! libheif, loaded on the first HEIC/HEIF or AVIF file: decodes the primary
//! image of the file, or another of its top-level images. The library is never linked at build time; without it,
//! or without a decoder for the file's codec, the file shows as broken with
//! the reason.
//!
//! The C API used here is the one of libheif 1.13 and newer, as declared in
//! `heif.h`. Its decoder plugins (libde265 for HEVC, dav1d or aom for AV1) are
//! registered by `heif_init`, which is called once and never undone.
#![expect(
    unsafe_code,
    reason = "calls into libheif, a C library loaded at runtime"
)]

use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr;
use std::sync::Arc;

use libloading::Library;

use crate::decode::DecodedImage;

/// Where the library is looked for, in this order.
pub const LIBRARY_NAMES: &[&str] = if cfg!(target_os = "macos") {
    &[
        "libheif.1.dylib",
        "/opt/homebrew/lib/libheif.1.dylib",
        "/usr/local/lib/libheif.1.dylib",
    ]
} else {
    &["libheif.so.1", "libheif.so"]
};

/// Major brands of the `ftyp` box of HEIF and AVIF files.
const BRANDS: &[&[u8; 4]] = &[
    b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx", b"mif1", b"msf1", b"avif", b"avis",
];

/// True when `bytes` start like a HEIF or AVIF file.
pub fn is_heif(bytes: &[u8]) -> bool {
    bytes.get(4..8) == Some(b"ftyp") && major_brand(bytes).is_some_and(|b| BRANDS.contains(&b))
}

fn major_brand(bytes: &[u8]) -> Option<&[u8; 4]> {
    bytes.get(8..12)?.try_into().ok()
}

/// `heif_error`, returned by value.
#[repr(C)]
struct HeifError {
    code: c_int,
    _subcode: c_int,
    /// Never null.
    message: *const c_char,
}

/// `heif_error_Ok`.
const OK: c_int = 0;
/// `heif_colorspace_RGB`.
const COLORSPACE_RGB: c_int = 1;
/// `heif_chroma_interleaved_RGBA`: 8 bits per channel.
const CHROMA_INTERLEAVED_RGBA: c_int = 11;
/// `heif_channel_interleaved`.
const CHANNEL_INTERLEAVED: c_int = 10;
/// `heif_compression_HEVC`.
const COMPRESSION_HEVC: c_int = 1;
/// `heif_compression_AV1`.
const COMPRESSION_AV1: c_int = 4;

type Context = *mut c_void;
type Handle = *mut c_void;
type Image = *mut c_void;
type Init = unsafe extern "C" fn(*mut c_void) -> HeifError;
type ContextAlloc = unsafe extern "C" fn() -> Context;
type ContextFree = unsafe extern "C" fn(Context);
type ReadFromMemory =
    unsafe extern "C" fn(Context, *const c_void, usize, *const c_void) -> HeifError;
/// `heif_item_id`.
type ItemId = u32;
type PrimaryImageId = unsafe extern "C" fn(Context, *mut ItemId) -> HeifError;
type TopLevelCount = unsafe extern "C" fn(Context) -> c_int;
type TopLevelIds = unsafe extern "C" fn(Context, *mut ItemId, c_int) -> c_int;
type ImageHandle = unsafe extern "C" fn(Context, ItemId, *mut Handle) -> HeifError;
type HandleRelease = unsafe extern "C" fn(Handle);
type DecodeImage =
    unsafe extern "C" fn(Handle, *mut Image, c_int, c_int, *const c_void) -> HeifError;
type ImageRelease = unsafe extern "C" fn(Image);
type ImageSize = unsafe extern "C" fn(Image, c_int) -> c_int;
type PlaneReadonly = unsafe extern "C" fn(Image, c_int, *mut c_int) -> *const u8;
type HaveDecoder = unsafe extern "C" fn(c_int) -> c_int;

/// The loaded and initialised library with the functions used.
pub struct Heif {
    context_alloc: ContextAlloc,
    context_free: ContextFree,
    read: ReadFromMemory,
    primary: PrimaryImageId,
    top_level_count: TopLevelCount,
    top_level_ids: TopLevelIds,
    image_handle: ImageHandle,
    handle_release: HandleRelease,
    decode: DecodeImage,
    image_release: ImageRelease,
    width: ImageSize,
    height: ImageSize,
    plane: PlaneReadonly,
    have_decoder: HaveDecoder,
    /// Keeps the functions above loaded.
    _library: Library,
}

/// Loads the first of `names` that exists and initialises it, which registers
/// its decoder plugins. The error is meant for the user.
pub fn load(names: &[&str]) -> Result<Heif, String> {
    let mut reason = String::new();
    for name in names {
        // SAFETY: libheif runs no initialisation code with preconditions when
        // loaded.
        match unsafe { Library::new(name) } {
            Ok(library) => return functions(library),
            Err(e) => reason = e.to_string(),
        }
    }
    Err(format!(
        "HEIC/HEIF and AVIF need libheif, which was not found: {reason}"
    ))
}

fn functions(library: Library) -> Result<Heif, String> {
    let missing =
        |e: libloading::Error| format!("HEIC/HEIF and AVIF need libheif 1.13 or newer: {e}");
    // SAFETY: the types match the declarations in `heif.h`, the library is
    // kept loaded as long as the pointers live, and `heif_init` takes null
    // for its default parameters.
    unsafe {
        let init = *library.get::<Init>(b"heif_init\0").map_err(missing)?;
        let heif = Heif {
            context_alloc: *library
                .get::<ContextAlloc>(b"heif_context_alloc\0")
                .map_err(missing)?,
            context_free: *library
                .get::<ContextFree>(b"heif_context_free\0")
                .map_err(missing)?,
            read: *library
                .get::<ReadFromMemory>(b"heif_context_read_from_memory_without_copy\0")
                .map_err(missing)?,
            primary: *library
                .get::<PrimaryImageId>(b"heif_context_get_primary_image_ID\0")
                .map_err(missing)?,
            top_level_count: *library
                .get::<TopLevelCount>(b"heif_context_get_number_of_top_level_images\0")
                .map_err(missing)?,
            top_level_ids: *library
                .get::<TopLevelIds>(b"heif_context_get_list_of_top_level_image_IDs\0")
                .map_err(missing)?,
            image_handle: *library
                .get::<ImageHandle>(b"heif_context_get_image_handle\0")
                .map_err(missing)?,
            handle_release: *library
                .get::<HandleRelease>(b"heif_image_handle_release\0")
                .map_err(missing)?,
            decode: *library
                .get::<DecodeImage>(b"heif_decode_image\0")
                .map_err(missing)?,
            image_release: *library
                .get::<ImageRelease>(b"heif_image_release\0")
                .map_err(missing)?,
            width: *library
                .get::<ImageSize>(b"heif_image_get_width\0")
                .map_err(missing)?,
            height: *library
                .get::<ImageSize>(b"heif_image_get_height\0")
                .map_err(missing)?,
            plane: *library
                .get::<PlaneReadonly>(b"heif_image_get_plane_readonly\0")
                .map_err(missing)?,
            have_decoder: *library
                .get::<HaveDecoder>(b"heif_have_decoder_for_format\0")
                .map_err(missing)?,
            _library: library,
        };
        check(init(ptr::null_mut())).map_err(|e| format!("libheif could not start: {e}"))?;
        Ok(heif)
    }
}

impl Heif {
    /// Top-level image `index` of the HEIF or AVIF file `bytes`, upright, and
    /// how many top-level images the file has. Image 0 is the primary one,
    /// the others follow in file order. libheif applies the file's rotation,
    /// mirroring and crop; the EXIF orientation is not applied on top, as it
    /// describes the same turn.
    pub fn decode(&self, bytes: &[u8], index: usize) -> Result<(DecodedImage, usize), String> {
        // SAFETY: the context is checked and freed once, after everything
        // taken from it; `bytes` outlives it, as reading without a copy needs.
        let result = unsafe {
            let context = (self.context_alloc)();
            if context.is_null() {
                return Err("libheif could not start".to_owned());
            }
            let result = self.decode_in(context, bytes, index);
            (self.context_free)(context);
            result
        };
        result.map_err(|e| self.missing_decoder(bytes).unwrap_or(e))
    }

    /// # Safety
    /// `context` is a live context; `bytes` outlives it.
    unsafe fn decode_in(
        &self,
        context: Context,
        bytes: &[u8],
        index: usize,
    ) -> Result<(DecodedImage, usize), String> {
        // SAFETY: see the function's contract; the handle is released once.
        unsafe {
            check((self.read)(
                context,
                bytes.as_ptr().cast(),
                bytes.len(),
                ptr::null(),
            ))?;
            let ids = self.image_ids(context)?;
            let id = ids
                .get(index)
                .ok_or_else(|| format!("the file has no image {}", index.saturating_add(1)))?;
            let mut handle: Handle = ptr::null_mut();
            check((self.image_handle)(context, *id, &mut handle))?;
            if handle.is_null() {
                return Err("libheif found no image".to_owned());
            }
            let result = self.decode_handle(handle);
            (self.handle_release)(handle);
            result.map(|image| (image, ids.len()))
        }
    }

    /// The top-level images of the file read into `context`, the primary one
    /// first.
    ///
    /// # Safety
    /// `context` is a live context holding a file.
    unsafe fn image_ids(&self, context: Context) -> Result<Vec<ItemId>, String> {
        // SAFETY: see the function's contract; the array holds as many IDs as
        // libheif is told it may fill.
        unsafe {
            let mut primary: ItemId = 0;
            check((self.primary)(context, &mut primary))?;
            let count = (self.top_level_count)(context);
            let mut ids: Vec<ItemId> = vec![0; usize::try_from(count).unwrap_or(0)];
            let filled = (self.top_level_ids)(context, ids.as_mut_ptr(), count);
            ids.truncate(usize::try_from(filled).unwrap_or(0));
            let others = ids.into_iter().filter(|id| *id != primary);
            Ok(std::iter::once(primary).chain(others).collect())
        }
    }

    /// # Safety
    /// `handle` is a live image handle.
    unsafe fn decode_handle(&self, handle: Handle) -> Result<DecodedImage, String> {
        // SAFETY: see the function's contract; null options keep the
        // defaults, which apply the transformations; the image is released
        // once.
        unsafe {
            let mut image: Image = ptr::null_mut();
            check((self.decode)(
                handle,
                &mut image,
                COLORSPACE_RGB,
                CHROMA_INTERLEAVED_RGBA,
                ptr::null(),
            ))?;
            if image.is_null() {
                return Err("libheif decoded no image".to_owned());
            }
            let result = self.pixels(image);
            (self.image_release)(image);
            result
        }
    }

    /// The RGBA pixels of a decoded image, without the row padding.
    ///
    /// # Safety
    /// `image` is a live image in interleaved RGBA.
    unsafe fn pixels(&self, image: Image) -> Result<DecodedImage, String> {
        let bad = || "libheif returned no pixels".to_owned();
        let mut stride: c_int = 0;
        // SAFETY: see the function's contract; each row read lies within the
        // plane of `height` rows of `stride` bytes, of which `width * 4` are
        // pixels.
        unsafe {
            let width = (self.width)(image, CHANNEL_INTERLEAVED);
            let height = (self.height)(image, CHANNEL_INTERLEAVED);
            let plane = (self.plane)(image, CHANNEL_INTERLEAVED, &mut stride);
            let w = usize::try_from(width).map_err(|_| bad())?;
            let h = usize::try_from(height).map_err(|_| bad())?;
            let stride = usize::try_from(stride).map_err(|_| bad())?;
            let row = w.checked_mul(4).ok_or_else(bad)?;
            if plane.is_null() || stride < row {
                return Err(bad());
            }
            let mut rgba = Vec::with_capacity(row.checked_mul(h).ok_or_else(bad)?);
            for y in 0..h {
                let start = plane.add(y.checked_mul(stride).ok_or_else(bad)?);
                rgba.extend_from_slice(std::slice::from_raw_parts(start, row));
            }
            Ok(DecodedImage {
                width: u32::try_from(width).map_err(|_| bad())?,
                height: u32::try_from(height).map_err(|_| bad())?,
                rgba: Arc::from(rgba),
            })
        }
    }

    /// Why `bytes` cannot be decoded when it is for lack of a decoder.
    fn missing_decoder(&self, bytes: &[u8]) -> Option<String> {
        let avif = matches!(major_brand(bytes), Some(b"avif" | b"avis"));
        let format = if avif {
            COMPRESSION_AV1
        } else {
            COMPRESSION_HEVC
        };
        // SAFETY: takes a plain enum value.
        if unsafe { (self.have_decoder)(format) } != 0 {
            return None;
        }
        Some(if avif {
            "AVIF needs libheif with an AV1 decoder (dav1d or aom), which was not found".to_owned()
        } else {
            "HEIC needs libheif with an HEVC decoder (libde265), which was not found".to_owned()
        })
    }
}

/// `Ok` for success, else the error's message.
fn check(error: HeifError) -> Result<(), String> {
    if error.code == OK {
        return Ok(());
    }
    if error.message.is_null() {
        return Err(format!("libheif failed with error {}", error.code));
    }
    // SAFETY: a non-null message is a NUL-terminated string.
    Err(unsafe { CStr::from_ptr(error.message) }
        .to_string_lossy()
        .into_owned())
}
