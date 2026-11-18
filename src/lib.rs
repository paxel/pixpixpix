//! pixpixpix: a fast, keyboard-first image and video viewer.

pub mod annotate;
pub mod app;
pub mod associations;
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
pub mod libraries;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod menu;
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
