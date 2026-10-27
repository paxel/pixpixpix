//! AppKit, which every Mac has: the general pasteboard for `c`, and the
//! application delegate through which Finder's "Open With" and a file dropped
//! on the Dock icon reach the app.
#![expect(
    unsafe_code,
    reason = "calls into AppKit through the Objective-C runtime"
)]

use std::sync::mpsc::Sender;

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::ProtocolObject;
use objc2::{AnyThread, DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationDelegate, NSApplicationWillFinishLaunchingNotification,
    NSPasteboard, NSPasteboardItem, NSPasteboardTypeFileURL, NSPasteboardTypePNG,
    NSPasteboardTypeString, NSPasteboardWriting,
};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSData, NSNotification, NSNotificationCenter, NSObject,
    NSObjectProtocol, NSString, NSURL,
};

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
    // SAFETY: NSObject has no subclassing requirements and `Delegate` does
    // not implement `Drop`.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[name = "PixpixpixDelegate"]
    #[ivars = Opener]
    struct Delegate;

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for Delegate {}

    // SAFETY: `NSApplicationDelegate` has no safety requirements.
    unsafe impl NSApplicationDelegate for Delegate {
        // SAFETY: the signature is the one AppKit calls.
        #[unsafe(method(application:openURLs:))]
        fn open_urls(&self, _application: &NSApplication, urls: &NSArray<NSURL>) {
            let opener = self.ivars();
            for path in urls.iter().filter_map(|url| url.to_file_path()) {
                // The app may already be gone; nothing to open then.
                let _ = opener.events.send(Event::Opened(path));
            }
            opener.ctx.request_repaint();
        }
    }

    impl Delegate {
        // SAFETY: the signature is the one a notification observer has.
        #[unsafe(method(willFinishLaunching:))]
        fn will_finish_launching(&self, _notification: &NSNotification) {
            // Winit sets no delegate of its own, and the files of the launch
            // are handed over only after this.
            NSApplication::sharedApplication(self.mtm())
                .setDelegate(Some(ProtocolObject::from_ref(self)));
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
    let delegate = Delegate::alloc(mtm).set_ivars(Opener { events, ctx });
    // SAFETY: NSObject's `init` has this signature.
    let delegate: Retained<Delegate> = unsafe { msg_send![super(delegate), init] };
    // SAFETY: the observer stays alive for good below, the selector takes
    // the notification, and the name is the constant AppKit defines.
    unsafe {
        NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &delegate,
            sel!(willFinishLaunching:),
            Some(NSApplicationWillFinishLaunchingNotification),
            None,
        );
    }
    // AppKit keeps neither the observer nor the delegate alive; they serve
    // as long as the app runs.
    std::mem::forget(delegate);
}
