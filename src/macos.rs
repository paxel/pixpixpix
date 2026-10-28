//! AppKit, which every Mac has: the general pasteboard for `c`, the Apple
//! Event through which Finder's "Open With" and a file dropped on the Dock
//! icon reach the app, and Launch Services for the default apps.
#![expect(
    unsafe_code,
    reason = "calls into AppKit through the Objective-C runtime"
)]

use std::sync::mpsc::Sender;

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::ProtocolObject;
use objc2::{AnyThread, DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplicationWillFinishLaunchingNotification, NSPasteboard, NSPasteboardItem,
    NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeString, NSPasteboardWriting,
};
use objc2_core_services::{kAEOpenDocuments, kCoreEventClass, keyDirectObject};
use objc2_foundation::{
    MainThreadMarker, NSAppleEventDescriptor, NSAppleEventManager, NSArray, NSData, NSNotification,
    NSNotificationCenter, NSObject, NSObjectProtocol, NSString, NSURL,
};

use objc2_app_kit::NSWorkspace;
use objc2_core_foundation::CFString;
use objc2_core_services::LSRolesMask;

use crate::clipboard::Content;
use crate::core::Event;

/// Puts `content` on the general pasteboard as one item: the PNG for image
/// apps, the file for Finder and plain text, each if there is one.
pub fn copy(content: &Content) -> Result<(), String> {
    // The clipboard thread has no pool of its own for what AppKit
    // autoreleases.
    autoreleasepool(|_| {
        let item = NSPasteboardItem::init(NSPasteboardItem::alloc());
        if !content.png.is_empty() {
            let png = NSData::with_bytes(&content.png);
            // SAFETY: the pasteboard type constants are immutable strings
            // AppKit defines.
            let png_type = unsafe { NSPasteboardTypePNG };
            if !item.setData_forType(&png, png_type) {
                return Err("the pasteboard refused the image".to_owned());
            }
        }
        if !content.text.is_empty() {
            let text = NSString::from_str(&content.text);
            // SAFETY: as above.
            let text_type = unsafe { NSPasteboardTypeString };
            if !item.setString_forType(&text, text_type) {
                return Err("the pasteboard refused the text".to_owned());
            }
        }
        let uri = content.uri_list.trim_end();
        if !uri.is_empty() {
            let url = NSString::from_str(uri);
            // SAFETY: as above.
            let url_type = unsafe { NSPasteboardTypeFileURL };
            if !item.setString_forType(&url, url_type) {
                return Err("the pasteboard refused the file".to_owned());
            }
        }
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        let items = NSArray::from_retained_slice(&[
            ProtocolObject::<dyn NSPasteboardWriting>::from_retained(item),
        ]);
        if pasteboard.writeObjects(&items) {
            Ok(())
        } else {
            Err("the pasteboard refused the image".to_owned())
        }
    })
}

/// Where the files AppKit asks to open go.
struct Opener {
    events: Sender<Event>,
    ctx: egui::Context,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and `Handler` does
    // not implement `Drop`.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[name = "PixpixpixOpenHandler"]
    #[ivars = Opener]
    struct Handler;

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for Handler {}

    impl Handler {
        // SAFETY: the signature is the one a notification observer has.
        #[unsafe(method(willFinishLaunching:))]
        fn will_finish_launching(&self, _notification: &NSNotification) {
            // Winit owns the application delegate and panics on any other,
            // so the files come from the Apple Event itself. AppKit installs
            // its own handler before this and sends the event only after.
            // SAFETY: `self` lives for good and has the selector, which takes
            // the event and the reply.
            unsafe {
                NSAppleEventManager::sharedAppleEventManager()
                    .setEventHandler_andSelector_forEventClass_andEventID(
                        self,
                        sel!(openDocuments:withReplyEvent:),
                        kCoreEventClass,
                        kAEOpenDocuments,
                    );
            }
        }

        // SAFETY: the signature is the one NSAppleEventManager calls.
        #[unsafe(method(openDocuments:withReplyEvent:))]
        fn open_documents(
            &self,
            event: &NSAppleEventDescriptor,
            _reply: &NSAppleEventDescriptor,
        ) {
            let opener = self.ivars();
            let Some(files) = event.paramDescriptorForKeyword(keyDirectObject) else {
                return;
            };
            // Descriptor lists count from 1.
            for index in 1..=files.numberOfItems() {
                let path = files
                    .descriptorAtIndex(index)
                    .and_then(|file| file.fileURLValue())
                    .and_then(|url| url.to_file_path());
                if let Some(path) = path {
                    // The app may already be gone; nothing to open then.
                    let _ = opener.events.send(Event::Opened(path));
                }
            }
            opener.ctx.request_repaint();
        }
    }
);

/// Sends the files AppKit asks to open, at launch or later, to `events` as
/// [`Event::Opened`] and wakes `ctx`. Call on the main thread before the
/// event loop runs; elsewhere it does nothing.
pub fn open_files(events: Sender<Event>, ctx: egui::Context) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let handler = Handler::alloc(mtm).set_ivars(Opener { events, ctx });
    // SAFETY: NSObject's `init` has this signature.
    let handler: Retained<Handler> = unsafe { msg_send![super(handler), init] };
    // SAFETY: the observer stays alive for good below, the selector takes
    // the notification, and the name is the constant AppKit defines.
    unsafe {
        NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &handler,
            sel!(willFinishLaunching:),
            Some(NSApplicationWillFinishLaunchingNotification),
            None,
        );
    }
    // AppKit keeps neither the observer nor the event handler alive; it
    // serves as long as the app runs.
    std::mem::forget(handler);
}

/// Launch Services, which keeps the default app of each content type.
pub struct LaunchServices;

impl crate::associations::Backend for LaunchServices {
    #[expect(
        deprecated,
        reason = "the NSWorkspace replacement needs macOS 12, the app runs from 11"
    )]
    fn default_for(&self, id: &str) -> Option<String> {
        let content_type = CFString::from_str(id);
        // SAFETY: a valid string and role; the result is owned.
        let handler = unsafe {
            objc2_core_services::LSCopyDefaultRoleHandlerForContentType(
                &content_type,
                LSRolesMask::All,
            )
        };
        handler.map(|handler| handler.to_string())
    }

    #[expect(
        deprecated,
        reason = "the NSWorkspace replacement needs macOS 12, the app runs from 11"
    )]
    fn set_default(&self, ids: &[&str], app: &str) -> Result<(), String> {
        let handler = CFString::from_str(app);
        for id in ids {
            let content_type = CFString::from_str(id);
            // SAFETY: valid strings and role, kept alive through the call.
            let status = unsafe {
                objc2_core_services::LSSetDefaultRoleHandlerForContentType(
                    &content_type,
                    LSRolesMask::All,
                    &handler,
                )
            };
            if status != 0 {
                return Err(format!("Launch Services refused {id} (error {status})"));
            }
        }
        Ok(())
    }

    fn app_name(&self, app: &str) -> String {
        let url = autoreleasepool(|_| {
            NSWorkspace::sharedWorkspace()
                .URLForApplicationWithBundleIdentifier(&NSString::from_str(app))
                .and_then(|url| url.to_file_path())
        });
        url.and_then(|path| Some(path.file_stem()?.to_string_lossy().into_owned()))
            .unwrap_or_else(|| app.to_owned())
    }

    fn ours(&self) -> String {
        crate::app::APP_ID.to_owned()
    }

    fn read_only(&self) -> bool {
        false
    }

    fn open_with(
        &self,
        app: &str,
        path: &std::path::Path,
    ) -> Result<std::process::Command, String> {
        let mut command = std::process::Command::new("open");
        command.arg("-b").arg(app).arg(path);
        Ok(command)
    }
}

/// Moves `path` to the Trash, returning where it went.
pub fn trash(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    autoreleasepool(|_| {
        let url = NSURL::from_file_path(path).ok_or("the path is not a file URL")?;
        let mut trashed: Option<Retained<NSURL>> = None;
        objc2_foundation::NSFileManager::defaultManager()
            .trashItemAtURL_resultingItemURL_error(&url, Some(&mut trashed))
            .map_err(|error| error.localizedDescription().to_string())?;
        trashed
            .and_then(|url| url.to_file_path())
            .ok_or_else(|| "the Trash did not say where the file went".to_owned())
    })
}
