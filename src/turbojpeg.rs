//! libturbojpeg, loaded on first use: lossless transforms of JPEG files in the
//! DCT domain, so turning a photo costs no quality. The library is never linked
//! at build time; without it the transform is an error the user sees.
//!
//! The C API used here is the classic TurboJPEG one of libjpeg-turbo 2.0 and
//! newer, as declared in `turbojpeg.h`.
#![expect(
    unsafe_code,
    reason = "calls into libturbojpeg, a C library loaded at runtime"
)]

use std::ffi::{CStr, c_char, c_int, c_uchar, c_ulong, c_void};
use std::ptr;

use libloading::Library;

/// Where the library is looked for, in this order.
pub const LIBRARY_NAMES: &[&str] = if cfg!(target_os = "macos") {
    &[
        "libturbojpeg.dylib",
        "libturbojpeg.0.dylib",
        "/opt/homebrew/opt/jpeg-turbo/lib/libturbojpeg.dylib",
        "/usr/local/opt/jpeg-turbo/lib/libturbojpeg.dylib",
    ]
} else {
    &["libturbojpeg.so.0", "libturbojpeg.so"]
};

/// A lossless transform of the stored pixels; turns are clockwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transform {
    None,
    FlipHorizontal,
    FlipVertical,
    /// Mirrored across the top-left to bottom-right diagonal.
    Transpose,
    /// Mirrored across the top-right to bottom-left diagonal.
    Transverse,
    Rotate90,
    Rotate180,
    Rotate270,
}

impl Transform {
    /// The `TJXOP_*` value of `turbojpeg.h`.
    fn op(self) -> c_int {
        match self {
            Self::None => 0,
            Self::FlipHorizontal => 1,
            Self::FlipVertical => 2,
            Self::Transpose => 3,
            Self::Transverse => 4,
            Self::Rotate90 => 5,
            Self::Rotate180 => 6,
            Self::Rotate270 => 7,
        }
    }
}

/// `TJXOPT_PERFECT`: fail rather than leave edge blocks untransformed.
const OPT_PERFECT: c_int = 1;
/// `TJXOPT_TRIM`: drop the edge blocks that cannot be transformed.
const OPT_TRIM: c_int = 2;

/// `tjregion`.
#[repr(C)]
struct Region {
    x: c_int,
    y: c_int,
    w: c_int,
    h: c_int,
}

/// `tjtransform`.
#[repr(C)]
struct TjTransform {
    r: Region,
    op: c_int,
    options: c_int,
    data: *mut c_void,
    /// A function pointer in C; always null here.
    custom_filter: *mut c_void,
}

type Handle = *mut c_void;
type InitTransform = unsafe extern "C" fn() -> Handle;
type TransformFn = unsafe extern "C" fn(
    Handle,
    *const c_uchar,
    c_ulong,
    c_int,
    *mut *mut c_uchar,
    *mut c_ulong,
    *mut TjTransform,
    c_int,
) -> c_int;
type GetErrorStr = unsafe extern "C" fn(Handle) -> *mut c_char;
type Free = unsafe extern "C" fn(*mut c_uchar);
type Destroy = unsafe extern "C" fn(Handle) -> c_int;

/// The loaded library with the functions used.
pub struct TurboJpeg {
    init: InitTransform,
    transform: TransformFn,
    error: GetErrorStr,
    free: Free,
    destroy: Destroy,
    /// Keeps the functions above loaded.
    _library: Library,
}

/// Loads the first of `names` that exists. The error is meant for the user.
pub fn load(names: &[&str]) -> Result<TurboJpeg, String> {
    let mut reason = String::new();
    for name in names {
        // SAFETY: libturbojpeg runs no initialisation code with
        // preconditions when loaded.
        match unsafe { Library::new(name) } {
            Ok(library) => return functions(library),
            Err(e) => reason = e.to_string(),
        }
    }
    Err(format!(
        "Lossless JPEG rotation needs libturbojpeg (libjpeg-turbo), which was not found: {reason}"
    ))
}

fn functions(library: Library) -> Result<TurboJpeg, String> {
    let missing = |e: libloading::Error| {
        format!("Lossless JPEG rotation needs libturbojpeg 2.0 or newer: {e}")
    };
    // SAFETY: the types match the declarations in `turbojpeg.h`, and the
    // library is kept loaded as long as the pointers live.
    unsafe {
        Ok(TurboJpeg {
            init: *library
                .get::<InitTransform>(b"tjInitTransform\0")
                .map_err(missing)?,
            transform: *library
                .get::<TransformFn>(b"tjTransform\0")
                .map_err(missing)?,
            error: *library
                .get::<GetErrorStr>(b"tjGetErrorStr2\0")
                .map_err(missing)?,
            free: *library.get::<Free>(b"tjFree\0").map_err(missing)?,
            destroy: *library.get::<Destroy>(b"tjDestroy\0").map_err(missing)?,
            _library: library,
        })
    }
}

impl TurboJpeg {
    /// The JPEG `jpeg` with `transform` applied losslessly; markers such as
    /// EXIF are copied unchanged. An image whose size is not a multiple of
    /// the JPEG block size loses the partial edge blocks it cannot turn.
    pub fn transform(&self, jpeg: &[u8], transform: Transform) -> Result<Vec<u8>, String> {
        self.run(jpeg, transform, OPT_PERFECT)
            .or_else(|_| self.run(jpeg, transform, OPT_TRIM))
    }

    fn run(&self, jpeg: &[u8], transform: Transform, options: c_int) -> Result<Vec<u8>, String> {
        let size = c_ulong::try_from(jpeg.len()).map_err(|e| e.to_string())?;
        let mut spec = TjTransform {
            r: Region {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            op: transform.op(),
            options,
            data: ptr::null_mut(),
            custom_filter: ptr::null_mut(),
        };
        let mut out: *mut c_uchar = ptr::null_mut();
        let mut out_size: c_ulong = 0;
        // SAFETY: the handle is checked and destroyed once; `jpeg` outlives
        // the call; the library allocates `out`, which is copied before it is
        // freed with the library's own `tjFree`.
        unsafe {
            let handle = (self.init)();
            if handle.is_null() {
                return Err("libturbojpeg could not start".to_owned());
            }
            let status = (self.transform)(
                handle,
                jpeg.as_ptr(),
                size,
                1,
                &mut out,
                &mut out_size,
                &mut spec,
                0,
            );
            let result = match usize::try_from(out_size) {
                Ok(len) if status == 0 && !out.is_null() => {
                    Ok(std::slice::from_raw_parts(out, len).to_vec())
                }
                _ => Err(error_text((self.error)(handle))),
            };
            if !out.is_null() {
                (self.free)(out);
            }
            (self.destroy)(handle);
            result
        }
    }
}

/// The text of a C error string, which may be null.
///
/// # Safety
/// `text` is null or points to a NUL-terminated string.
unsafe fn error_text(text: *const c_char) -> String {
    if text.is_null() {
        return "libturbojpeg failed".to_owned();
    }
    // SAFETY: see the function's contract.
    unsafe { CStr::from_ptr(text) }
        .to_string_lossy()
        .into_owned()
}
