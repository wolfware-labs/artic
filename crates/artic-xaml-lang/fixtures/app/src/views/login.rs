//! A code type: the element at the root of `login.xaml`.

use artic::controls::TextBlock;
use artic::prelude::Handle;

/// The sign-in form.
#[artic::ui_class("login.xaml")]
#[derive(Debug)]
pub struct LoginView {
  /// How many times sign-in was tried.
  pub attempts: u32,
}

impl LoginView {
  /// The status line, a named node only this module reaches by default.
  pub fn status(&self) -> Handle<TextBlock> {
    self.ui.status
  }
}
