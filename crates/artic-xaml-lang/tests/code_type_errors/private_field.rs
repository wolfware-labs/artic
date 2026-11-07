//! A named node without a field visibility, read outside the code type's module.

mod form {
  #[artic::ui_class("private_field.xaml")]
  #[derive(Debug)]
  pub struct LoginView;
}

fn main() {
  let mut tree = artic::prelude::UiTree::new();
  let login = form::LoginView::create(&mut tree);
  let _ = login.get(&tree).ui.email;
}
