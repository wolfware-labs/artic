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

/// What a view needs in scope: typed handles and the builder methods of the property groups
/// every element has, such as `.margin(…)` and `.width(…)`.
pub mod prelude {
  pub use artic_controls::PanelMethods;
  pub use artic_core::tree::Handle;
  pub use artic_element::{AppearanceMethods, IdentityMethods, InteractionMethods, LayoutMethods};
}

#[cfg(feature = "xaml")]
pub mod xaml;
