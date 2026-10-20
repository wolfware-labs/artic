//! artic: a cross-platform UI framework with XAML views and Rust logic.
//!
//! This facade re-exports the public API. The crates with UI and value types are its modules:
//! [`controls`], [`element`], [`layout`] and [`render`].
//!
//! Features:
//!
//! - `hot-reload`: reload views from disk during development.
//! - `runtime-xaml`: load XAML files at runtime in release builds.
//! - `skia`: use the `skia-safe` rendering backend.
//! - `xaml`: `xaml::namespaces()`, the namespace URIs XAML views use, for the runtime loader and
//!   for `build.rs`.
//! - `xaml-compiler`: `xaml::compile_dir`, which compiles a directory of XAML views to Rust from
//!   `build.rs`. Turns `xaml` on.

#[doc(hidden)]
pub use artic_element::__private;

/// The controls and the panel group.
pub use artic_controls as controls;
/// The element contract, the property groups every element has, and the surfaces.
pub use artic_element as element;
/// Layout and the geometry and text values properties take, such as `Thickness`.
pub use artic_layout as layout;
/// Scenes, colors, brushes and shadows.
pub use artic_render as render;

/// What a view needs in scope: the tree, typed handles and the builder methods of the property
/// groups every element has, such as `.margin(…)` and `.width(…)`.
pub mod prelude {
  pub use artic_controls::PanelMethods;
  pub use artic_core::tree::{Handle, UiTree};
  pub use artic_element::{AppearanceMethods, IdentityMethods, InteractionMethods, LayoutMethods};
}

#[cfg(feature = "xaml")]
pub mod xaml;
