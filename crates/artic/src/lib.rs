//! artic: a cross-platform UI framework with XAML views and Rust logic.
//!
//! This facade re-exports the public API. Features:
//!
//! - `hot-reload`: reload views from disk during development.
//! - `runtime-xaml`: load XAML files at runtime in release builds.
//! - `skia`: use the `skia-safe` rendering backend.
//! - `xaml`: `xaml::namespaces()`, the namespace URIs XAML views use, for the runtime loader and
//!   for `build.rs`.

#[doc(hidden)]
pub use artic_element::__private;

#[cfg(feature = "xaml")]
pub mod xaml;
