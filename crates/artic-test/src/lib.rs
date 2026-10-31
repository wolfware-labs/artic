//! Test utilities for artic: golden images, headless layout, shared scenes and the test text engine.
//! A headless host (`TestHost`, #101) and tree serialization (#60) are planned.

pub use artic_core::reactive::{LiveCounts, assert_no_reactive_leaks, live_counts};

pub mod golden;
pub mod layout;
pub mod require;
pub mod scenes;
pub mod text;

/// Whether the environment variable `name` is `1`.
fn flag(name: &str) -> bool {
  std::env::var_os(name).is_some_and(|value| value == "1")
}

#[doc(hidden)]
pub mod __private {
  pub use accesskit;
  pub use artic_element::{AccessCx, Element, RenderCx, element_type};
  pub use artic_render::SceneBuilder;
}
