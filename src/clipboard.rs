//! The clipboard: puts the shown pixels and the file itself on it in one go,
//! so pasting into an image app gives the picture and pasting into a file
//! manager copies the file. On Linux pixpixpix owns the X11 clipboard from a
//! window of its own and serves it as long as it runs; Wayland sessions reach
//! it through XWayland. macOS is not supported yet.

use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, ImageFormat, RgbaImage};

use crate::decode::DecodedImage;
use crate::folder;
use crate::rotate::Rotation;

/// What goes on the clipboard, in every format offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Content {
    /// The pixels as shown, PNG encoded (`image/png`).
    pub png: Vec<u8>,
    /// The file for file managers that read `text/uri-list`.
    pub uri_list: String,
    /// The file for GNOME and KDE file managers
    /// (`x-special/gnome-copied-files`).
    pub gnome_files: String,
}

/// The clipboard content for the file at `path` shown as `image` turned by
/// `rotation`.
pub fn content(path: &Path, image: &DecodedImage, rotation: Rotation) -> Result<Content, String> {
    let uri = file_uri(path)?;
    Ok(Content {
        png: png(&turned(image, rotation)?)?,
        uri_list: format!("{uri}\r\n"),
        gnome_files: format!("copy\n{uri}"),
    })
}

/// The pixels turned by `rotation`, the same way a written rotation turns
/// them.
pub fn turned(image: &DecodedImage, rotation: Rotation) -> Result<DecodedImage, String> {
    let rgba = RgbaImage::from_raw(image.width, image.height, image.rgba.to_vec())
        .ok_or("the pixels do not match the image size")?;
    let image = DynamicImage::ImageRgba8(rgba);
    Ok(DecodedImage::from_dynamic(match rotation {
        Rotation::None => image,
        Rotation::Right => image.rotate90(),
        Rotation::Half => image.rotate180(),
        Rotation::Left => image.rotate270(),
    }))
}

/// The pixels encoded as PNG.
pub fn png(image: &DecodedImage) -> Result<Vec<u8>, String> {
    let rgba = RgbaImage::from_raw(image.width, image.height, image.rgba.to_vec())
        .ok_or("the pixels do not match the image size")?;
    let mut out = Cursor::new(Vec::new());
    rgba.write_to(&mut out, ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

/// The `file://` URI of `path`, made absolute, with every byte outside the
/// unreserved characters and `/` percent-encoded.
pub fn file_uri(path: &Path) -> Result<String, String> {
    let path = std::path::absolute(path).map_err(|e| e.to_string())?;
    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_encoded_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    Ok(uri)
}

/// The system clipboard, connected on the first copy.
#[derive(Default)]
pub struct Clipboard {
    #[cfg(all(unix, not(target_os = "macos")))]
    owner: Option<x11::Owner>,
}

impl Clipboard {
    /// Puts the file at `path`, shown as `image` turned by `rotation`, on the
    /// clipboard. The error names the file and is meant to be shown to the
    /// user as is.
    pub fn copy(
        &mut self,
        path: &Path,
        image: &DecodedImage,
        rotation: Rotation,
    ) -> Result<(), String> {
        content(path, image, rotation)
            .and_then(|content| self.set(content))
            .map_err(|e| format!("Cannot copy {}: {e}", folder::file_name(path)))
    }

    /// Puts only `image` on the clipboard, without a file: the pixels of an
    /// export. The error is meant to be shown to the user as is.
    pub fn copy_pixels(&mut self, image: &DecodedImage) -> Result<(), String> {
        png(image)
            .map(|png| Content {
                png,
                uri_list: String::new(),
                gnome_files: String::new(),
            })
            .and_then(|content| self.set(content))
            .map_err(|e| format!("Cannot copy the export: {e}"))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn set(&mut self, content: Content) -> Result<(), String> {
        // A connection that broke is made again.
        if self.owner.as_ref().is_some_and(x11::Owner::stopped) {
            self.owner = None;
        }
        let owner = match &mut self.owner {
            Some(owner) => owner,
            None => self.owner.insert(x11::Owner::connect()?),
        };
        owner.set(content)
    }

    #[cfg(not(all(unix, not(target_os = "macos"))))]
    fn set(&mut self, _content: Content) -> Result<(), String> {
        Err("the clipboard is not supported on this system yet".to_owned())
    }
}

/// The X11 clipboard owner: a window that holds the CLIPBOARD selection and a
/// thread that answers the requests for it. Content too large for one request
/// goes out in INCR chunks.
#[cfg(all(unix, not(target_os = "macos")))]
mod x11 {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, PoisonError};
    use std::thread::JoinHandle;

    use x11rb::connection::{Connection, RequestConnection as _};
    use x11rb::protocol::Event;
    use x11rb::protocol::xproto::{
        Atom, AtomEnum, ChangeWindowAttributesAux, ConnectionExt as _, CreateWindowAux, EventMask,
        PropMode, Property, PropertyNotifyEvent, SELECTION_NOTIFY_EVENT, SelectionNotifyEvent,
        SelectionRequestEvent, Time, Window, WindowClass,
    };
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;
    use x11rb::{COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT, NONE};

    use super::Content;

    x11rb::atom_manager! {
        Atoms: AtomCookies {
            CLIPBOARD,
            TARGETS,
            ATOM,
            INCR,
            PNG: b"image/png",
            URI_LIST: b"text/uri-list",
            GNOME_FILES: b"x-special/gnome-copied-files",
        }
    }

    /// The size of one INCR chunk.
    const CHUNK: usize = 256 * 1024;

    /// What the serving thread shares with the owner.
    struct Shared {
        conn: RustConnection,
        window: Window,
        atoms: Atoms,
        /// The bytes of each offered target.
        targets: Mutex<Vec<(Atom, Arc<[u8]>)>>,
    }

    pub struct Owner {
        shared: Arc<Shared>,
        server: JoinHandle<()>,
    }

    impl Owner {
        /// Connects to the X server of `DISPLAY` and starts serving.
        pub fn connect() -> Result<Self, String> {
            let (conn, screen) = RustConnection::connect(None)
                .map_err(|e| format!("no X11 display to reach the clipboard ({e})"))?;
            let root = conn
                .setup()
                .roots
                .get(screen)
                .map(|s| s.root)
                .ok_or("the X11 display has no screen")?;
            let window = conn.generate_id().map_err(|e| e.to_string())?;
            conn.create_window(
                COPY_DEPTH_FROM_PARENT,
                window,
                root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_OUTPUT,
                COPY_FROM_PARENT,
                &CreateWindowAux::new(),
            )
            .map_err(|e| e.to_string())?;
            let atoms = Atoms::new(&conn)
                .map_err(|e| e.to_string())?
                .reply()
                .map_err(|e| e.to_string())?;
            let shared = Arc::new(Shared {
                conn,
                window,
                atoms,
                targets: Mutex::new(Vec::new()),
            });
            let serving = Arc::clone(&shared);
            let server = std::thread::spawn(move || serve(&serving));
            Ok(Self { shared, server })
        }

        /// Whether the serving thread ended, with the connection gone.
        pub fn stopped(&self) -> bool {
            self.server.is_finished()
        }

        /// Offers `content` and takes the clipboard; fine only once owned.
        pub fn set(&self, content: Content) -> Result<(), String> {
            let shared = &self.shared;
            let atoms = &shared.atoms;
            // Formats without content, a file for pixels only, are not offered.
            *shared
                .targets
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = [
                (atoms.PNG, content.png),
                (atoms.URI_LIST, content.uri_list.into_bytes()),
                (atoms.GNOME_FILES, content.gnome_files.into_bytes()),
            ]
            .into_iter()
            .filter(|(_, bytes)| !bytes.is_empty())
            .map(|(atom, bytes)| (atom, Arc::from(bytes)))
            .collect();
            let conn = &shared.conn;
            conn.set_selection_owner(shared.window, atoms.CLIPBOARD, Time::CURRENT_TIME)
                .map_err(|e| e.to_string())?;
            let owner = conn
                .get_selection_owner(atoms.CLIPBOARD)
                .map_err(|e| e.to_string())?
                .reply()
                .map_err(|e| e.to_string())?
                .owner;
            if owner == shared.window {
                Ok(())
            } else {
                Err("another program holds the clipboard".to_owned())
            }
        }
    }

    /// An INCR transfer under way: what is left to send to a requestor.
    struct Transfer {
        target: Atom,
        bytes: Arc<[u8]>,
        sent: usize,
    }

    /// Answers requests until the connection breaks.
    fn serve(shared: &Shared) {
        let mut transfers: HashMap<(Window, Atom), Transfer> = HashMap::new();
        while let Ok(event) = shared.conn.wait_for_event() {
            // A request that cannot be answered fails for that requestor
            // only; the next one may work.
            let _ = match event {
                Event::SelectionRequest(request) => answer(shared, &request, &mut transfers),
                Event::PropertyNotify(notify) => send_chunk(shared, &notify, &mut transfers),
                _ => Ok(()),
            };
        }
    }

    /// Answers one request: the list of targets, the bytes of one, or the
    /// start of an INCR transfer for bytes too large for one request.
    fn answer(
        shared: &Shared,
        request: &SelectionRequestEvent,
        transfers: &mut HashMap<(Window, Atom), Transfer>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (conn, atoms) = (&shared.conn, &shared.atoms);
        // Obsolete clients name no property; the target is used then.
        let property = if request.property == NONE {
            request.target
        } else {
            request.property
        };
        let targets = shared
            .targets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let found = targets.iter().find(|(atom, _)| *atom == request.target);
        let answered = if request.selection != atoms.CLIPBOARD {
            false
        } else if request.target == atoms.TARGETS {
            let mut list: Vec<Atom> = targets.iter().map(|(atom, _)| *atom).collect();
            list.push(atoms.TARGETS);
            conn.change_property32(
                PropMode::REPLACE,
                request.requestor,
                property,
                atoms.ATOM,
                &list,
            )?;
            true
        } else if let Some((target, bytes)) = found {
            if bytes.len() < conn.maximum_request_bytes().saturating_sub(1024) {
                conn.change_property8(
                    PropMode::REPLACE,
                    request.requestor,
                    property,
                    *target,
                    bytes,
                )?;
            } else {
                // The requestor deleting the property asks for the next chunk.
                conn.change_window_attributes(
                    request.requestor,
                    &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
                )?;
                let size = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
                conn.change_property32(
                    PropMode::REPLACE,
                    request.requestor,
                    property,
                    atoms.INCR,
                    &[size],
                )?;
                transfers.insert(
                    (request.requestor, property),
                    Transfer {
                        target: *target,
                        bytes: Arc::clone(bytes),
                        sent: 0,
                    },
                );
            }
            true
        } else {
            false
        };
        conn.send_event(
            false,
            request.requestor,
            EventMask::NO_EVENT,
            SelectionNotifyEvent {
                response_type: SELECTION_NOTIFY_EVENT,
                sequence: 0,
                time: request.time,
                requestor: request.requestor,
                selection: request.selection,
                target: request.target,
                property: if answered {
                    property
                } else {
                    AtomEnum::NONE.into()
                },
            },
        )?;
        conn.flush()?;
        Ok(())
    }

    /// Sends the next chunk of an INCR transfer once the requestor took the
    /// last one; an empty chunk ends it.
    fn send_chunk(
        shared: &Shared,
        notify: &PropertyNotifyEvent,
        transfers: &mut HashMap<(Window, Atom), Transfer>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if notify.state != Property::DELETE {
            return Ok(());
        }
        let key = (notify.window, notify.atom);
        let Some(transfer) = transfers.get_mut(&key) else {
            return Ok(());
        };
        let conn = &shared.conn;
        let end = transfer
            .sent
            .saturating_add(CHUNK)
            .min(transfer.bytes.len());
        let chunk = transfer.bytes.get(transfer.sent..end).unwrap_or_default();
        conn.change_property8(
            PropMode::REPLACE,
            notify.window,
            notify.atom,
            transfer.target,
            chunk,
        )?;
        if chunk.is_empty() {
            transfers.remove(&key);
            conn.change_window_attributes(
                notify.window,
                &ChangeWindowAttributesAux::new().event_mask(EventMask::NO_EVENT),
            )?;
        } else {
            transfer.sent = end;
        }
        conn.flush()?;
        Ok(())
    }
}
