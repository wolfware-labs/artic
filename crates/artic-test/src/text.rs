//! The text engine artic's tests shape with.

use artic_text::Engine;

/// An engine that knows only the test font, Atkinson Hyperlegible, regular and bold, so text
/// measures the same on every machine and in every crate's tests.
#[must_use]
pub fn engine() -> Engine {
  Engine::with_fonts([artic_test_fonts::REGULAR, artic_test_fonts::BOLD])
}
