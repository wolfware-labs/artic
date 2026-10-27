//! Layout tests without a window or a GPU: build a tree in a [`Headless`], run
//! [`LayoutManager`](artic_layout::LayoutManager), and check sizes and rectangles. `Headless`
//! finds every element registered with `artic_element::element_type!`, the controls and the
//! shared test elements [`Fixed`] and [`Stack`] alike.
//!
//! ```
//! use artic_layout::geometry::{Rect, Size};
//! use artic_test::layout::{Fixed, Headless, Stack};
//!
//! let mut layout = Headless::new();
//! let first = layout.add("first", Fixed::new(100.0, 20.0));
//! let second = layout.add("second", Fixed::new(80.0, 30.0));
//! let root = layout.parent("stack", Stack::new(), &[first, second]);
//!
//! layout.run(root, Size::new(400.0, 300.0), 1.0);
//!
//! layout.assert_desired(root, Size::new(100.0, 50.0));
//! layout.assert_rect(second, Rect::new(0.0, 20.0, 400.0, 30.0));
//! ```
//!
//! A failure names the node by its label: `"second": rect expected 0,20,400,30, got 0,20,400,24`.
//! [`strategy`] feeds `proptest` with available sizes and scales, and [`Invariants`] checks the
//! rules a layout keeps for any of them.

mod compare;
mod elements;
mod headless;
mod invariants;
pub mod strategy;

pub use elements::{Fixed, Stack};
pub use headless::Headless;
pub use invariants::Invariants;

/// How far apart, in logical units, two layout values may be and still match.
pub const TOLERANCE: f32 = 0.001;
