//! libmpv, loaded on the first video: a player per video, drawing its frames
//! into memory through the software render API. The library is never linked
//! at build time; without it a video shows as broken with the reason.
//!
//! The C API used here is the one of `client.h` and `render.h` as of mpv
//! v0.41.0 (client API 2.5); every function, struct and value used is the same
//! in v0.34.1 (client API 1.109, `libmpv.so.1`), which is looked for too.
//!
//! Once a player has its render context, the thread driving it calls only the
//! functions `render.h` allows on a render thread: the `mpv_render_*` ones and
//! those `client.h` marks "Safe to be called from mpv render API threads".
#![expect(
    unsafe_code,
    reason = "calls into libmpv, a C library loaded at runtime"
)]

use std::ffi::{CStr, CString, OsStr, c_char, c_int, c_void};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;

use libloading::Library;

use crate::decode::DecodedImage;

/// Where the library is looked for, in this order, after the one bundled in
/// `pixpixpix.app` on macOS.
pub const LIBRARY_NAMES: &[&str] = if cfg!(target_os = "macos") {
    &[
        "libmpv.2.dylib",
        "/opt/homebrew/lib/libmpv.2.dylib",
        "/usr/local/lib/libmpv.2.dylib",
    ]
} else {
    &["libmpv.so.2", "libmpv.so.1"]
};

/// The library in `pixpixpix.app`, as the release packs it.
const BUNDLED: &str = "libmpv.2.dylib";

/// Where libmpv is looked for, in this order: on macOS the one bundled in the
/// app first, then [`LIBRARY_NAMES`].
pub fn library_paths() -> Vec<PathBuf> {
    let bundled = if cfg!(target_os = "macos") {
        std::env::current_exe().ok().and_then(|exe| bundled(&exe))
    } else {
        None
    };
    bundled
        .into_iter()
        .chain(LIBRARY_NAMES.iter().map(PathBuf::from))
        .collect()
}

/// The libmpv of an app bundle for its executable `exe`:
/// `Contents/MacOS/<exe>` finds `Contents/Frameworks/libmpv.2.dylib`. The
/// executable is resolved first, as the cask's `pixpixpix` and `pix3`
/// commands are links into the app. None when `exe` does not exist.
pub fn bundled(exe: &Path) -> Option<PathBuf> {
    let exe = std::fs::canonicalize(exe).ok()?;
    let contents = exe.parent()?.parent()?;
    Some(contents.join("Frameworks").join(BUNDLED))
}

/// `mpv_event`.
#[repr(C)]
struct MpvEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

/// The leading fields of `mpv_event_end_file`; only they are read.
#[repr(C)]
struct EndFile {
    reason: c_int,
    error: c_int,
}

/// `mpv_event_property`.
#[repr(C)]
struct EventProperty {
    _name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

/// `mpv_render_param`.
#[repr(C)]
struct RenderParam {
    kind: c_int,
    data: *mut c_void,
}

/// `MPV_EVENT_NONE`.
const EVENT_NONE: c_int = 0;
/// `MPV_EVENT_COMMAND_REPLY`.
const EVENT_COMMAND_REPLY: c_int = 5;
/// `MPV_EVENT_END_FILE`.
const EVENT_END_FILE: c_int = 7;
/// `MPV_EVENT_PLAYBACK_RESTART`.
const EVENT_PLAYBACK_RESTART: c_int = 21;
/// `MPV_EVENT_PROPERTY_CHANGE`.
const EVENT_PROPERTY_CHANGE: c_int = 22;
/// `MPV_END_FILE_REASON_EOF`.
const END_FILE_REASON_EOF: c_int = 0;
/// `MPV_END_FILE_REASON_ERROR`.
const END_FILE_REASON_ERROR: c_int = 4;
/// `MPV_FORMAT_STRING`.
const FORMAT_STRING: c_int = 1;
/// `MPV_FORMAT_INT64`.
const FORMAT_INT64: c_int = 4;
/// `MPV_FORMAT_DOUBLE`.
const FORMAT_DOUBLE: c_int = 5;
/// `MPV_RENDER_PARAM_INVALID`, which ends a parameter list.
const PARAM_END: c_int = 0;
/// `MPV_RENDER_PARAM_API_TYPE`.
const PARAM_API_TYPE: c_int = 1;
/// `MPV_RENDER_PARAM_SW_SIZE`.
const PARAM_SW_SIZE: c_int = 17;
/// `MPV_RENDER_PARAM_SW_FORMAT`.
const PARAM_SW_FORMAT: c_int = 18;
/// `MPV_RENDER_PARAM_SW_STRIDE`.
const PARAM_SW_STRIDE: c_int = 19;
/// `MPV_RENDER_PARAM_SW_POINTER`.
const PARAM_SW_POINTER: c_int = 20;
/// `MPV_RENDER_API_TYPE_SW`.
const API_TYPE_SW: &CStr = c"sw";
/// 4 bytes per pixel, red first; the fourth byte is garbage.
const SW_FORMAT: &CStr = c"rgb0";
/// `MPV_RENDER_UPDATE_FRAME`.
const UPDATE_FRAME: u64 = 1;

/// The `reply_userdata` of the observed display width and height.
const WIDTH_ID: u64 = 1;
const HEIGHT_ID: u64 = 2;
/// The `reply_userdata` of commands sent with [`Player::command`].
const COMMAND_ID: u64 = 3;
/// The `reply_userdata` of the observed playback position.
const POSITION_ID: u64 = 4;
/// The `reply_userdata` of the observed rotation the file asks for.
const ROTATION_ID: u64 = 5;

type Handle = *mut c_void;
type RenderContext = *mut c_void;
type Callback = unsafe extern "C" fn(*mut c_void);
type Create = unsafe extern "C" fn() -> Handle;
type SetOptionString = unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int;
type Initialize = unsafe extern "C" fn(Handle) -> c_int;
type TerminateDestroy = unsafe extern "C" fn(Handle);
type ErrorString = unsafe extern "C" fn(c_int) -> *const c_char;
type ObserveProperty = unsafe extern "C" fn(Handle, u64, *const c_char, c_int) -> c_int;
type CommandAsync = unsafe extern "C" fn(Handle, u64, *mut *const c_char) -> c_int;
type SetPropertyAsync =
    unsafe extern "C" fn(Handle, u64, *const c_char, c_int, *mut c_void) -> c_int;
type WaitEvent = unsafe extern "C" fn(Handle, f64) -> *mut MpvEvent;
type SetWakeupCallback = unsafe extern "C" fn(Handle, Option<Callback>, *mut c_void);
type RenderCreate = unsafe extern "C" fn(*mut RenderContext, Handle, *mut RenderParam) -> c_int;
type RenderSetUpdateCallback = unsafe extern "C" fn(RenderContext, Option<Callback>, *mut c_void);
type RenderUpdate = unsafe extern "C" fn(RenderContext) -> u64;
type Render = unsafe extern "C" fn(RenderContext, *mut RenderParam) -> c_int;
type RenderFree = unsafe extern "C" fn(RenderContext);

/// The loaded library with the functions used.
pub struct Mpv {
    create: Create,
    set_option: SetOptionString,
    initialize: Initialize,
    terminate_destroy: TerminateDestroy,
    error_string: ErrorString,
    observe: ObserveProperty,
    command: CommandAsync,
    set_property: SetPropertyAsync,
    wait_event: WaitEvent,
    set_wakeup: SetWakeupCallback,
    render_create: RenderCreate,
    render_set_update: RenderSetUpdateCallback,
    render_update: RenderUpdate,
    render: Render,
    render_free: RenderFree,
    /// Keeps the functions above loaded.
    _library: Library,
}

/// Loads the first of `names` that exists. The error is meant for the user.
pub fn load<P: AsRef<OsStr>>(names: &[P]) -> Result<Mpv, String> {
    let mut reason = String::new();
    for name in names {
        // SAFETY: libmpv runs no initialisation code with preconditions when
        // loaded.
        match unsafe { Library::new(name) } {
            Ok(library) => return functions(library),
            Err(e) => reason = e.to_string(),
        }
    }
    Err(format!("Videos need libmpv, which was not found: {reason}"))
}

fn functions(library: Library) -> Result<Mpv, String> {
    let missing = |e: libloading::Error| format!("Videos need libmpv 0.34 or newer: {e}");
    // SAFETY: the types match the declarations in `client.h` and `render.h`,
    // and the library is kept loaded as long as the pointers live.
    unsafe {
        Ok(Mpv {
            create: *library.get::<Create>(b"mpv_create\0").map_err(missing)?,
            set_option: *library
                .get::<SetOptionString>(b"mpv_set_option_string\0")
                .map_err(missing)?,
            initialize: *library
                .get::<Initialize>(b"mpv_initialize\0")
                .map_err(missing)?,
            terminate_destroy: *library
                .get::<TerminateDestroy>(b"mpv_terminate_destroy\0")
                .map_err(missing)?,
            error_string: *library
                .get::<ErrorString>(b"mpv_error_string\0")
                .map_err(missing)?,
            observe: *library
                .get::<ObserveProperty>(b"mpv_observe_property\0")
                .map_err(missing)?,
            command: *library
                .get::<CommandAsync>(b"mpv_command_async\0")
                .map_err(missing)?,
            set_property: *library
                .get::<SetPropertyAsync>(b"mpv_set_property_async\0")
                .map_err(missing)?,
            wait_event: *library
                .get::<WaitEvent>(b"mpv_wait_event\0")
                .map_err(missing)?,
            set_wakeup: *library
                .get::<SetWakeupCallback>(b"mpv_set_wakeup_callback\0")
                .map_err(missing)?,
            render_create: *library
                .get::<RenderCreate>(b"mpv_render_context_create\0")
                .map_err(missing)?,
            render_set_update: *library
                .get::<RenderSetUpdateCallback>(b"mpv_render_context_set_update_callback\0")
                .map_err(missing)?,
            render_update: *library
                .get::<RenderUpdate>(b"mpv_render_context_update\0")
                .map_err(missing)?,
            render: *library
                .get::<Render>(b"mpv_render_context_render\0")
                .map_err(missing)?,
            render_free: *library
                .get::<RenderFree>(b"mpv_render_context_free\0")
                .map_err(missing)?,
            _library: library,
        })
    }
}

/// What a player reports, read with [`Player::next_event`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerEvent {
    /// The display width of the video changed; absent while unknown.
    Width(Option<u32>),
    /// The display height of the video changed; absent while unknown.
    Height(Option<u32>),
    /// The playback position changed; absent while unknown.
    Position(Option<Duration>),
    /// The file asks for its picture to be turned clockwise by these
    /// degrees, which [`Player::render`] does not do.
    Rotation(u32),
    /// The file ended, with mpv's message when it could not be played.
    Ended(Option<String>),
    /// Playback restarted after a seek, or after the file was loaded.
    Restarted,
    /// A [`Player::command`] failed, with mpv's message.
    CommandFailed(String),
    /// Anything else.
    Other,
}

/// One mpv instance with a software render context. It sends `wake` on
/// `wakeup` whenever it has news: an event or a frame. Not to be shared
/// between threads, but it may move to another one.
pub struct Player {
    mpv: &'static Mpv,
    handle: Handle,
    render: RenderContext,
    /// The callbacks' context, alive until the handle is destroyed.
    wakeup: Box<Sender<()>>,
}

// SAFETY: the handle and render context may be used from any thread, one at
// a time; `Player` is not `Sync`, so it is.
unsafe impl Send for Player {}

/// The callback of both the wakeup and the render update: tells the thread
/// driving the player that there is news.
unsafe extern "C" fn wake(context: *mut c_void) {
    // SAFETY: `context` is the player's boxed sender, which outlives the
    // callbacks.
    let sender = unsafe { &*context.cast::<Sender<()>>() };
    // The driving thread may be gone already; nothing to wake then.
    let _ = sender.send(());
}

impl Mpv {
    /// Starts a player with the options `name=value`, which then wakes
    /// `wakeup` on news. Options it does not know are an error.
    pub fn player(
        &'static self,
        options: &[(&str, &str)],
        wakeup: Sender<()>,
    ) -> Result<Player, String> {
        // SAFETY: a fresh handle is set up before it is initialised; on any
        // failure it is destroyed once, by the player's drop.
        unsafe {
            let handle = (self.create)();
            if handle.is_null() {
                return Err("mpv could not start".to_owned());
            }
            let mut player = Player {
                mpv: self,
                handle,
                render: ptr::null_mut(),
                wakeup: Box::new(wakeup),
            };
            for (name, value) in std::iter::once(&("vo", "libmpv")).chain(options) {
                let (name, value) = (c_string(name)?, c_string(value)?);
                self.check((self.set_option)(handle, name.as_ptr(), value.as_ptr()))?;
            }
            self.check((self.initialize)(handle))?;
            let observed = [
                (WIDTH_ID, c"dwidth"),
                (HEIGHT_ID, c"dheight"),
                (ROTATION_ID, c"video-dec-params/rotate"),
            ];
            for (id, name) in observed {
                self.check((self.observe)(handle, id, name.as_ptr(), FORMAT_INT64))?;
            }
            self.check((self.observe)(
                handle,
                POSITION_ID,
                c"time-pos".as_ptr(),
                FORMAT_DOUBLE,
            ))?;
            let mut params = [
                RenderParam {
                    kind: PARAM_API_TYPE,
                    data: API_TYPE_SW.as_ptr().cast_mut().cast(),
                },
                RenderParam {
                    kind: PARAM_END,
                    data: ptr::null_mut(),
                },
            ];
            self.check((self.render_create)(
                &mut player.render,
                handle,
                params.as_mut_ptr(),
            ))?;
            let context: *mut Sender<()> = &mut *player.wakeup;
            (self.render_set_update)(player.render, Some(wake), context.cast());
            (self.set_wakeup)(handle, Some(wake), context.cast());
            Ok(player)
        }
    }

    /// `Ok` for success, else mpv's message for the error code.
    fn check(&self, code: c_int) -> Result<(), String> {
        if code >= 0 {
            return Ok(());
        }
        Err(self.message(code))
    }

    fn message(&self, code: c_int) -> String {
        // SAFETY: takes any code and returns a static string, or null.
        let text = unsafe { (self.error_string)(code) };
        if text.is_null() {
            return format!("mpv failed with error {code}");
        }
        // SAFETY: a non-null result is a NUL-terminated string.
        unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned()
    }
}

impl Player {
    /// Starts playing the file, replacing any other.
    pub fn load_file(&self, path: &Path) -> Result<(), String> {
        let path = CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|_| "the file name holds a NUL byte".to_owned())?;
        let mut args = [c"loadfile".as_ptr(), path.as_ptr(), ptr::null()];
        // SAFETY: a NULL-terminated list of strings that live through the
        // call, which copies them.
        let code = unsafe { (self.mpv.command)(self.handle, 0, args.as_mut_ptr()) };
        self.mpv.check(code)
    }

    /// Runs a command, such as `seek 10 relative`, without waiting for it; a
    /// failure is reported as [`PlayerEvent::CommandFailed`].
    pub fn command(&self, args: &[&CStr]) -> Result<(), String> {
        let mut args: Vec<*const c_char> = args
            .iter()
            .map(|a| a.as_ptr())
            .chain(std::iter::once(ptr::null()))
            .collect();
        // SAFETY: a NULL-terminated list of strings that live through the
        // call, which copies them.
        let code = unsafe { (self.mpv.command)(self.handle, COMMAND_ID, args.as_mut_ptr()) };
        self.mpv.check(code)
    }

    /// Sets a property, such as `pause` or `mute`, without waiting for it.
    pub fn set(&self, name: &CStr, value: &CStr) -> Result<(), String> {
        let mut value = value.as_ptr();
        // SAFETY: a string property is passed as a pointer to the string,
        // which the call copies.
        let code = unsafe {
            (self.mpv.set_property)(
                self.handle,
                0,
                name.as_ptr(),
                FORMAT_STRING,
                (&raw mut value).cast(),
            )
        };
        self.mpv.check(code)
    }

    /// The next event, without waiting; `None` once none is left.
    pub fn next_event(&self) -> Option<PlayerEvent> {
        // SAFETY: never null; the event and its data stay valid until the next
        // call, and are read before it.
        unsafe {
            let event = &*(self.mpv.wait_event)(self.handle, 0.0);
            match event.event_id {
                EVENT_NONE => None,
                EVENT_END_FILE if !event.data.is_null() => {
                    let end = &*event.data.cast::<EndFile>();
                    Some(match end.reason {
                        END_FILE_REASON_ERROR => {
                            PlayerEvent::Ended(Some(self.mpv.message(end.error)))
                        }
                        END_FILE_REASON_EOF => PlayerEvent::Ended(None),
                        _ => PlayerEvent::Other,
                    })
                }
                EVENT_PLAYBACK_RESTART => Some(PlayerEvent::Restarted),
                EVENT_COMMAND_REPLY if event.reply_userdata == COMMAND_ID && event.error < 0 => {
                    Some(PlayerEvent::CommandFailed(self.mpv.message(event.error)))
                }
                EVENT_PROPERTY_CHANGE
                    if !event.data.is_null() && event.reply_userdata == POSITION_ID =>
                {
                    let property = &*event.data.cast::<EventProperty>();
                    let position = (property.format == FORMAT_DOUBLE && !property.data.is_null())
                        .then(|| *property.data.cast::<f64>())
                        .and_then(|seconds| Duration::try_from_secs_f64(seconds).ok());
                    Some(PlayerEvent::Position(position))
                }
                EVENT_PROPERTY_CHANGE if !event.data.is_null() => {
                    let property = &*event.data.cast::<EventProperty>();
                    let value = (property.format == FORMAT_INT64 && !property.data.is_null())
                        .then(|| *property.data.cast::<i64>())
                        .and_then(|v| u32::try_from(v).ok());
                    let size = value.filter(|v| *v > 0);
                    Some(match event.reply_userdata {
                        WIDTH_ID => PlayerEvent::Width(size),
                        HEIGHT_ID => PlayerEvent::Height(size),
                        ROTATION_ID => PlayerEvent::Rotation(value.unwrap_or(0)),
                        _ => PlayerEvent::Other,
                    })
                }
                _ => Some(PlayerEvent::Other),
            }
        }
    }

    /// Whether a new frame waits to be drawn with [`Player::render`].
    pub fn frame_ready(&self) -> bool {
        // SAFETY: the render context is live.
        unsafe { (self.mpv.render_update)(self.render) & UPDATE_FRAME != 0 }
    }

    /// Draws the current frame scaled to `width` by `height`; blocks until it
    /// is due while playing.
    pub fn render(&self, width: u32, height: u32) -> Result<DecodedImage, String> {
        let bad = || "the video is too large".to_owned();
        let (w, h) = (
            usize::try_from(width).map_err(|_| bad())?,
            usize::try_from(height).map_err(|_| bad())?,
        );
        let mut size = [
            c_int::try_from(width).map_err(|_| bad())?,
            c_int::try_from(height).map_err(|_| bad())?,
        ];
        let mut stride: usize = w.checked_mul(4).ok_or_else(bad)?;
        // Whole `u32`s, so every row starts aligned to a pixel as mpv needs.
        let mut pixels: Vec<u32> = vec![0; w.checked_mul(h).ok_or_else(bad)?];
        let mut params = [
            RenderParam {
                kind: PARAM_SW_SIZE,
                data: size.as_mut_ptr().cast(),
            },
            RenderParam {
                kind: PARAM_SW_FORMAT,
                data: SW_FORMAT.as_ptr().cast_mut().cast(),
            },
            RenderParam {
                kind: PARAM_SW_STRIDE,
                data: (&raw mut stride).cast(),
            },
            RenderParam {
                kind: PARAM_SW_POINTER,
                data: pixels.as_mut_ptr().cast(),
            },
            RenderParam {
                kind: PARAM_END,
                data: ptr::null_mut(),
            },
        ];
        // SAFETY: the surface holds `height` rows of `stride` bytes, and all
        // parameters live through the call.
        let code = unsafe { (self.mpv.render)(self.render, params.as_mut_ptr()) };
        self.mpv.check(code)?;
        let mut rgba = Vec::with_capacity(stride.checked_mul(h).ok_or_else(bad)?);
        for pixel in pixels {
            let [r, g, b, _] = pixel.to_ne_bytes();
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
        Ok(DecodedImage {
            width,
            height,
            rgba: Arc::from(rgba),
        })
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // SAFETY: the render context goes before the core, as `render.h`
        // demands; both are freed once. The callbacks' sender is dropped only
        // after, with the fields.
        unsafe {
            if !self.render.is_null() {
                (self.mpv.render_free)(self.render);
            }
            (self.mpv.terminate_destroy)(self.handle);
        }
    }
}

fn c_string(text: &str) -> Result<CString, String> {
    CString::new(text).map_err(|_| format!("bad mpv option {text}"))
}
