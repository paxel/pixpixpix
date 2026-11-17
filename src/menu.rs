//! The items of the macOS app menu and what each one sends to the core. The
//! menu itself is AppKit's and lives in `macos`; this part is the same on
//! every system, so it is tested on every system.

use crate::core::{Event, Key, KeyPress};

/// An item of the app menu that the app answers itself; Hide is AppKit's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    About,
    Settings,
    Quit,
}

impl Item {
    /// What the menu calls the item.
    pub fn title(self) -> &'static str {
        match self {
            Item::About => "About pixpixpix",
            Item::Settings => "Settings…",
            Item::Quit => "Quit pixpixpix",
        }
    }

    /// The key that, with `Cmd`, chooses the item; empty without one.
    pub fn key_equivalent(self) -> &'static str {
        match self {
            Item::About => "",
            Item::Settings => ",",
            Item::Quit => "q",
        }
    }

    /// The event the item sends: the press of the key that does the same,
    /// so the menu can do nothing a key cannot, and waits where keys wait.
    pub fn event(self) -> Event {
        Event::Key(match self {
            Item::About => KeyPress::plain(Key::F1),
            Item::Settings => KeyPress::plain(Key::S),
            Item::Quit => KeyPress::command(Key::Q),
        })
    }
}
