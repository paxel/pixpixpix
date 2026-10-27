//! Helper processes: pixpixpix started again with [`FLAG`] and the name of a
//! job runs libmpv away from the app, so a crash inside a native library ends
//! only the helper. Messages go both ways over the helper's stdin and stdout,
//! each a little-endian `u32` length followed by that many bytes. A helper
//! ends once its stdin closes, so none outlives the app.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::Arc;

use crate::decode::DecodedImage;
use crate::{grabber, libraries, video};

/// The first argument that makes pixpixpix a helper.
pub const FLAG: &str = "--helper";

/// The job of a helper playing one video.
pub const PLAY: &str = "play";

/// The job of the helper grabbing first frames.
pub const GRAB: &str = "grab";

/// The job of the helper looking for the optional libraries.
pub const LIBRARIES: &str = "libraries";

/// Runs the helper job the arguments name, if they name one; `None` for a
/// normal start.
pub fn serve() -> Option<ExitCode> {
    let mut args = std::env::args_os().skip(1);
    if args.next()? != FLAG {
        return None;
    }
    let job = args.next();
    Some(match job.as_ref().and_then(|j| j.to_str()) {
        Some(PLAY) => video::serve_playback(),
        Some(GRAB) => grabber::serve(),
        Some(LIBRARIES) => libraries::serve(),
        _ => ExitCode::FAILURE,
    })
}

/// Whether what native libraries print is shown: `PIXPIXPIX_DEBUG=1`.
pub fn debug() -> bool {
    std::env::var_os("PIXPIXPIX_DEBUG").is_some_and(|value| value == "1")
}

/// The command that starts the app as the helper for `job`.
pub fn command(job: &str) -> io::Result<Command> {
    let mut command = Command::new(app_executable()?);
    command.args([FLAG, job]);
    Ok(command)
}

/// This executable, or for a test binary, which Cargo puts into `deps`, the
/// app binary one folder up.
fn app_executable() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let app = |name: &std::ffi::OsStr| name == "pixpixpix" || name == "pix3";
    if exe.file_stem().is_some_and(app) {
        return Ok(exe);
    }
    let beside = exe.parent().and_then(Path::parent).map(|dir| {
        dir.join("pixpixpix")
            .with_extension(std::env::consts::EXE_EXTENSION)
    });
    Ok(beside.filter(|path| path.exists()).unwrap_or(exe))
}

/// Starts `command` with its stdin and stdout piped to the app; what it
/// prints to stderr is dropped unless [`debug`].
pub fn spawn(mut command: Command) -> io::Result<Child> {
    let stderr = if debug() {
        Stdio::inherit()
    } else {
        Stdio::null()
    };
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(stderr)
        .spawn()
}

/// Writes one message and flushes it.
pub fn write_message(out: &mut impl Write, message: &[u8]) -> io::Result<()> {
    let length = u32::try_from(message.len()).map_err(|_| io::Error::other("message too long"))?;
    out.write_all(&length.to_le_bytes())?;
    out.write_all(message)?;
    out.flush()
}

/// Reads one message; `None` once the other side closed between messages.
pub fn read_message(input: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut length = [0; 4];
    match input.read_exact(&mut length) {
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        result => result?,
    }
    let length = usize::try_from(u32::from_le_bytes(length)).map_err(io::Error::other)?;
    let mut message = vec![0; length];
    input.read_exact(&mut message)?;
    Ok(Some(message))
}

/// Builds a message field by field.
pub struct Writer(Vec<u8>);

impl Writer {
    pub fn new(tag: u8) -> Self {
        Self(vec![tag])
    }

    pub fn u8(mut self, value: u8) -> Self {
        self.0.push(value);
        self
    }

    pub fn u32(mut self, value: u32) -> Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn i32(mut self, value: i32) -> Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn u64(mut self, value: u64) -> Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// The bytes, which must be the last field: they run to the end.
    pub fn rest(mut self, bytes: &[u8]) -> Vec<u8> {
        self.0.extend_from_slice(bytes);
        self.0
    }

    /// The image, which must be the last field.
    pub fn image(self, image: &DecodedImage) -> Vec<u8> {
        self.u32(image.width).u32(image.height).rest(&image.rgba)
    }

    pub fn done(self) -> Vec<u8> {
        self.0
    }
}

/// Reads a message field by field; `None` for a message too short.
pub struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    pub fn new(message: &'a [u8]) -> Self {
        Self(message)
    }

    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, tail) = self.0.split_first_chunk::<N>()?;
        self.0 = tail;
        Some(*head)
    }

    pub fn u8(&mut self) -> Option<u8> {
        self.take::<1>().map(|[byte]| byte)
    }

    pub fn u32(&mut self) -> Option<u32> {
        self.take().map(u32::from_le_bytes)
    }

    pub fn i32(&mut self) -> Option<i32> {
        self.take().map(i32::from_le_bytes)
    }

    pub fn u64(&mut self) -> Option<u64> {
        self.take().map(u64::from_le_bytes)
    }

    /// Everything left.
    pub fn rest(self) -> &'a [u8] {
        self.0
    }

    /// The image written by [`Writer::image`]; `None` if its pixels do not
    /// match its size.
    pub fn image(mut self) -> Option<DecodedImage> {
        let (width, height) = (self.u32()?, self.u32()?);
        let rgba = self.rest();
        let expected = u64::from(width)
            .checked_mul(u64::from(height))?
            .checked_mul(4)?;
        (u64::try_from(rgba.len()).ok()? == expected).then(|| DecodedImage {
            width,
            height,
            rgba: Arc::from(rgba),
        })
    }
}

/// The bytes of a path, to send it.
#[cfg(unix)]
pub fn path_bytes(path: &Path) -> &[u8] {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes()
}

/// The path sent as [`path_bytes`].
#[cfg(unix)]
pub fn path_from(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
pub fn path_bytes(path: &Path) -> &[u8] {
    path.to_str().unwrap_or_default().as_bytes()
}

#[cfg(not(unix))]
pub fn path_from(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}
