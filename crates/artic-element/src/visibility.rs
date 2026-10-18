//! How much of an element is shown, and the pointer over it.

use artic_macros::UiEnum;

/// How much of an element is shown.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, UiEnum)]
pub enum Visibility {
  /// Drawn, hit-tested and laid out.
  #[default]
  Visible,
  /// Takes its space but isn't drawn, hit-tested or described to assistive technology.
  Hidden,
  /// Takes no space, and its children aren't laid out.
  Collapsed,
}

/// The pointer shown over an element.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, UiEnum)]
pub enum Cursor {
  /// The usual arrow.
  #[default]
  Arrow,
  /// A pointing hand, over something clickable.
  Hand,
  /// A text cursor, over text that can be selected or edited.
  Text,
  /// Busy: the application is working.
  Wait,
  /// A crosshair, for precise picking.
  Crosshair,
  /// The action isn't allowed here.
  NotAllowed,
  /// Something can be moved.
  Move,
  /// Something can be resized sideways.
  ResizeHorizontal,
  /// Something can be resized up and down.
  ResizeVertical,
}
