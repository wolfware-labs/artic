use artic_macros::ui_class;

#[ui_class("login.xaml")]
#[derive(Debug)]
struct LoginView<T> {
  value: T,
}

fn main() {}
