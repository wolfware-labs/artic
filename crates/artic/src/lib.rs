//! artic: a cross-platform UI framework with views in a UI language and logic in Rust.
//!
//! This facade re-exports the public API. The crates with UI and value types are its modules:
//! [`controls`], [`core`], [`element`](mod@element), [`layout`] and [`render`]. With default
//! features it adds no UI language, host or renderer: each comes with a feature named after it.
//!
//! An app crate's `pub fn app() -> artic::App` says how to build its tree, and a launcher runs it
//! with [`run`], naming a host and a renderer.
//!
//! [`ui_class`] makes a struct the code type of a compiled view: the element at its root, holding
//! its named nodes. The root also has what an app's own code writes: the reactive types, the tree
//! and its handles, [`element`](macro@element) and the element traits, and the `UiType` and
//! `UiEnum` derives.
//!
//! Features:
//!
//! - `view-compiler`: `view::compile_dir` and `view::crate_paths()`, which compile a directory of
//!   views of any UI language to Rust from `build.rs`.
//! - `xaml`: `xaml::namespaces()`, the namespace URIs XAML views use, for `build.rs`.
//! - `xaml-compiler`: `xaml::compile_dir`, the short form for XAML views. Turns `xaml` and
//!   `view-compiler` on.

#[doc(hidden)]
pub use artic_element::__private;

/// The controls and the panel group.
pub use artic_controls as controls;
/// The tree, its handles and the reactive properties.
pub use artic_core as core;
/// The element contract, the property groups every element has, and the surfaces.
pub use artic_element as element;
/// Layout and the geometry and text values properties take, such as `Thickness`.
pub use artic_layout as layout;
/// Scenes, colors, brushes and shadows.
pub use artic_render as render;

/// Makes a struct the code type of a compiled view, the element at the root of the view.
pub use artic_macros::ui_class;

pub use app::{App, run};
pub use artic_core::property::Property;
pub use artic_core::reactive::{Memo, Signal, effect};
pub use artic_core::tree::{Handle, NodeId, UiTree};
pub use artic_element::{Element, ElementImpl, ElementKind};
pub use artic_macros::{UiEnum, UiType, element};
pub use artic_platform::Error;
pub use artic_registry::UiType;

/// What a view needs in scope: the tree, typed handles and the builder methods of the property
/// groups every element has, such as `.margin(…)` and `.width(…)`.
pub mod prelude {
  pub use artic_controls::PanelMethods;
  pub use artic_core::tree::{Handle, UiTree};
  pub use artic_element::{AppearanceMethods, IdentityMethods, InteractionMethods, LayoutMethods};
}

mod app;
mod hooks;
#[cfg(feature = "view-compiler")]
pub mod view;
#[cfg(feature = "xaml")]
pub mod xaml;
