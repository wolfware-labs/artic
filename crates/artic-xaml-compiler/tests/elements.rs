// Resolving element types and the shape of the generated module: one `let` per element and a
// `View` holding the root.

mod common;

use common::{compile, compile_with};

// A bare element becomes a `View` whose root is its handle, built with the type's builder through
// the facade's path.
#[test]
fn bare_element_builds_its_root() {
  let rust = compile(r#"<Border xmlns="https://artic.rs/ui"/>"#).expect("a valid view");
  insta::assert_snapshot!(rust, @r"
  // Generated from view.xaml.

  use ::artic::prelude::{Handle, UiTree};
  /// The nodes of `view.xaml`: its root and one handle per `x:Name`.
  #[allow(non_snake_case)]
  #[derive(Clone, Copy, Debug)]
  pub struct View {
      pub root: Handle<::artic::controls::Border>,
  }
  /// Builds `view.xaml` in `tree` and returns its nodes.
  pub fn build(tree: &mut UiTree) -> View {
      let __e0 = ::artic::controls::Border::new().build(tree);
      View { root: __e0 }
  }
  ");
}

// A type no crate of the namespace has is reported at its name, naming the namespace.
#[test]
fn unknown_element_reports_error() {
  let errors = compile(r#"<Gauge xmlns="https://artic.rs/ui"/>"#).expect_err("no such type");
  assert!(errors.contains("no type `Gauge` in `https://artic.rs/ui`"), "{errors}");
}

// The header and the doc comments name the file with `/` separators on every platform.
#[test]
fn file_path_uses_forward_slashes() {
  let rust = compile_with(r#"<Border xmlns="https://artic.rs/ui"/>"#, "views/account/login.xaml", |_| {}).expect("a valid view");
  assert!(rust.starts_with("// Generated from views/account/login.xaml.\n"), "{rust}");
  assert!(rust.contains("/// The nodes of `views/account/login.xaml`"), "{rust}");
}
