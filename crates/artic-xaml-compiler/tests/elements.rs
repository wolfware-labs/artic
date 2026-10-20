// Resolving element types and the shape of the generated module: one `let` per element and a
// `View` holding the root.

mod common;

use artic_core::property::Property;
use artic_macros::UiType;
use common::{TEST_UI, compile, compile_with};

/// A test type sharing `Border`'s name, so a URI standing for both crates finds two.
#[derive(Debug, UiType)]
struct Border {
  #[ui]
  width: Property<f32>,
}

/// A value provider, which an attribute value names and an element can't.
#[derive(Debug, UiType)]
#[ui(value_provider)]
struct Lookup;

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

// A name two crates of one URI define is ambiguous, and the help lists both paths.
#[test]
fn name_in_two_crates_is_ambiguous() {
  let errors = compile_with(r#"<Border xmlns="https://artic.rs/ui"/>"#, "view.xaml", |map| {
    map.alias("https://artic.rs/ui", &[env!("CARGO_CRATE_NAME")]);
  })
  .expect_err("two Borders");
  assert!(errors.contains("`Border` is ambiguous in `https://artic.rs/ui`"), "{errors}");
  assert!(errors.contains("`artic_controls::border::Border`, `elements::Border`"), "{errors}");
}

// A `using:` type is written with the module path the document gave, its crate translated
// through the map.
#[test]
fn using_type_keeps_its_module_path() {
  let rust = compile(r#"<c:Border xmlns:c="using:artic_controls::border"/>"#).expect("a valid view");
  assert!(rust.contains("::artic::controls::border::Border::new()"), "{rust}");
}

// A `using:` path with no type there is unknown, naming the namespace.
#[test]
fn using_path_without_type_reports_error() {
  let errors = compile(r#"<c:Gauge xmlns:c="using:artic_controls::border"/>"#).expect_err("no Gauge");
  assert!(errors.contains("no type `Gauge` in `using:artic_controls::border`"), "{errors}");
}

// A namespace that points at the crate whose build script runs gets the build-dependency help.
#[test]
fn app_crate_type_gets_build_dependency_help() {
  let errors = compile(r#"<l:Gauge xmlns:l="using:artic_xaml_compiler::views"/>"#).expect_err("no Gauge");
  assert!(errors.contains("add that crate to `[build-dependencies]`"), "{errors}");
}

// A test crate's own type resolves through its URI, at the crate's root.
#[test]
fn test_crate_type_resolves() {
  let rust = compile(&format!(r#"<Border xmlns="{TEST_UI}"/>"#)).expect("a valid view");
  assert!(rust.contains("::elements::Border::new()"), "{rust}");
}

// An `x:` primitive written as an element comes with #62.
#[test]
fn primitive_element_reports_error() {
  let errors = compile(r#"<x:String xmlns:x="https://artic.rs/markup">a</x:String>"#).expect_err("a primitive");
  assert!(errors.contains("XAML primitives come with #62"), "{errors}");
}

// A value provider can't be an element, the root included.
#[test]
fn value_provider_element_reports_error() {
  let errors = compile(&format!(r#"<Lookup xmlns="{TEST_UI}"/>"#)).expect_err("a value provider");
  assert!(errors.contains("`Lookup` is a value provider, not an element"), "{errors}");
}

// Every unknown type is reported, not only the first.
#[test]
fn every_unknown_type_is_reported() {
  let errors = compile(r#"<Border xmlns="https://artic.rs/ui"><Gauge/></Border>"#).expect_err("unknown child");
  assert!(errors.contains("no type `Gauge`"), "{errors}");
  let errors = compile(r#"<StackPanel xmlns="https://artic.rs/ui"><Gauge/><Dial/></StackPanel>"#).expect_err("two unknown");
  assert!(errors.contains("no type `Gauge`") && errors.contains("no type `Dial`"), "{errors}");
}
