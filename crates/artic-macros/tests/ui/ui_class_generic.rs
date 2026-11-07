use artic_macros::ui_class;

#[ui_class("login.ui")]
#[derive(Debug)]
struct LoginView<T> {
  value: T,
}

fn main() {}
