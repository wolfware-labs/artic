//! [`UserControl`], the root of a view an app composes from other controls.

use artic_element::{Child, ElementImpl};
use artic_macros::element;

/// The root of a view: a control an app builds from other controls, usually described in a UI language
/// and view-model. Its content fills it, and it adds nothing to what assistive technology sees.
#[element]
#[ui(code_type_root)]
#[derive(Debug)]
pub struct UserControl {
  /// The content, which fills the view.
  #[ui(content)]
  pub child: Child,
}

impl ElementImpl for UserControl {}
