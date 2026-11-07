use artic_macros::ui_class;

// trybuild builds this file as part of another crate, so the view is outside that crate.
#[ui_class("login.ui")]
#[derive(Debug)]
struct LoginView;

fn main() {}
