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

pub mod app;
pub mod core;
pub mod decode;
pub mod external;
pub mod folder;
pub mod geometry;
pub mod settings;
pub mod startup;
pub mod ui;
pub mod watch;

pub use app::run;
