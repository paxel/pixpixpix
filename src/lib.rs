//! pixpixpix: a fast, keyboard-first image and video viewer.

// Shipped code may not index, slice or do arithmetic that can panic either; the
// unwrap/expect/panic part of the policy lives in Cargo.toml. Unit tests are
// exempt via `cfg(test)`; integration tests are separate crates.
#![cfg_attr(
    not(test),
    deny(
        clippy::unreachable,
        clippy::indexing_slicing,
        clippy::string_slice,
        clippy::arithmetic_side_effects
    )
)]

pub mod annotate;
pub mod app;
pub mod clipboard;
pub mod core;
pub mod decode;
pub mod external;
pub mod folder;
pub mod geometry;
pub mod grabber;
pub mod heif;
pub mod helper;
pub mod launcher;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod metadata;
pub mod motion;
pub mod mpv;
pub mod multi;
pub mod report;
pub mod rotate;
pub mod settings;
pub mod startup;
pub mod thumbnails;
pub mod trash;
pub mod turbojpeg;
pub mod ui;
pub mod video;
pub mod watch;

pub use app::run;
