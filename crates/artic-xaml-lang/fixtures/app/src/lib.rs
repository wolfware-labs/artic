//! An app whose views its build script compiles, for the compiler's integration tests.

/// The compiled views.
pub mod views {
  pub mod app;
  /// `src/views/card.xaml`.
  pub mod card {
    include!(concat!(env!("OUT_DIR"), "/views/src/views/card.rs"));
  }

  pub mod login;
}

use artic::controls::TextBlock;
use artic::prelude::Handle;
use views::login::LoginView;

/// The email field, a `pub(crate)` named node reached from outside the code type's module.
pub fn email(view: &LoginView) -> Handle<TextBlock> {
  view.ui.email
}
