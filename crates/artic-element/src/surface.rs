//! The roots a host presents: an [`Application`] and its [`Surface`]s, and the [`Desktop`] settings
//! a desktop host applies to them.

use crate::{AccessCx, ElementImpl};
use artic_core::property::Property;
use artic_layout::geometry::{Rect, Size};
use artic_layout::{ArrangeCx, MeasureCx};
use artic_macros::{UiEnum, UiType, element};

/// The root of an app's tree. Its logical children are the [`Surface`]s the host presents, and a
/// [`Desktop`] setting on it reaches every one of them.
#[element]
#[derive(Debug)]
pub struct Application;

impl ElementImpl for Application {}

/// A root the host presents: a window on a desktop, the whole screen on a phone. Its content
/// fills it.
///
/// Only an [`Application`]'s logical children are presented. Settings that mean something to one
/// platform family are attached properties of that family's owner, such as [`Desktop`].
#[element]
#[derive(Debug)]
pub struct Surface {
  /// What the surface is called: its window's title, and its name to assistive technology.
  #[ui(affects = surface | render)]
  pub title: Property<String>,
  /// Whether the host presents it. The user closing its window sets it to `false`; setting it
  /// to `true` again presents it again.
  #[ui(default = true, affects = surface)]
  pub is_open: Property<bool>,
}

impl ElementImpl for Surface {
  fn measure_override(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
    cx.measure_children(available)
  }

  fn arrange_override(&self, cx: &mut ArrangeCx<'_>, size: Size) {
    cx.arrange_children(Rect::new(0.0, 0.0, size.width, size.height));
  }

  fn accessibility(&self, _cx: &AccessCx<'_>, node: &mut accesskit::Node) {
    node.set_role(accesskit::Role::Window);
    node.set_label(self.title.get());
  }
}

/// The settings a desktop host applies to a [`Surface`]'s window, as attached properties.
///
/// Each inherits: set on the [`Application`], it reaches every surface; a style on `Surface` or a
/// local value overrides it for some. A host that isn't a desktop never reads them.
#[derive(Debug, UiType)]
#[ui(attached(name = "Width", ty = Option<f32>, affects = surface, target = any, inherits))]
#[ui(attached(name = "Height", ty = Option<f32>, affects = surface, target = any, inherits))]
#[ui(attached(name = "SizeToContent", ty = SizeToContent, affects = surface, target = any, inherits))]
#[ui(attached(name = "Chrome", ty = Chrome, affects = surface, target = any, inherits))]
pub struct Desktop;

/// Which axes of a window its content sizes. The user can't resize those axes.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, UiEnum)]
pub enum SizeToContent {
  /// Neither: the window has the size `Desktop.Width` and `Desktop.Height` give, or the host's.
  #[default]
  Manual,
  /// The width.
  Width,
  /// The height.
  Height,
  /// Both.
  WidthAndHeight,
}

impl SizeToContent {
  /// Whether content sizes the width.
  #[must_use]
  pub const fn width(self) -> bool {
    matches!(self, SizeToContent::Width | SizeToContent::WidthAndHeight)
  }

  /// Whether content sizes the height.
  #[must_use]
  pub const fn height(self) -> bool {
    matches!(self, SizeToContent::Height | SizeToContent::WidthAndHeight)
  }
}

/// Who draws a window's frame.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, UiEnum)]
pub enum Chrome {
  /// The OS draws the title bar and borders.
  #[default]
  System,
  /// The app draws everything.
  None,
}
