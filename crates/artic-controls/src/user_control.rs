//! [`UserControl`], the root of a view an app composes from other controls.

use artic_element::ElementImpl;
use artic_layout::geometry::{Rect, Size};
use artic_layout::{ArrangeCx, MeasureCx};
use artic_macros::element;

/// The root of a view: a control an app builds from other controls, usually described in a UI language
/// and view-model. Its content fills it, and it adds nothing to what assistive technology sees.
#[element]
#[derive(Debug)]
pub struct UserControl;

impl ElementImpl for UserControl {
  fn measure_override(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
    cx.measure_children(available)
  }

  fn arrange_override(&self, cx: &mut ArrangeCx<'_>, size: Size) {
    cx.arrange_children(Rect::new(0.0, 0.0, size.width, size.height));
  }
}
