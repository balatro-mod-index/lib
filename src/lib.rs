#![deny(
    missing_debug_implementations,
    rust_2018_idioms,
    clippy::complexity,
    clippy::correctness
)]
#![warn(clippy::perf, clippy::pedantic)]
// TODO: add ALL THE DOCS
#![allow(clippy::missing_errors_doc)]

pub mod forge;
pub mod mods;

#[cfg(feature = "lfs")]
pub mod lfs;
