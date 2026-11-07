use artic_macros::ui_class;

#[ui_class("login.xaml")]
#[derive(Debug)]
struct LoginView {
  child: u32,
}

fn main() {}
