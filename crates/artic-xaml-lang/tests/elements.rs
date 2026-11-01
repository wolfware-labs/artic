// Resolving element types and the shape of the generated module: one `let` per element and a
// `View` holding the root.

mod common;

use artic_core::property::Property;
use artic_macros::UiType;
use artic_xaml_lang::Xaml;
use common::{TEST_UI, compile, compile_as, compile_with};

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

  use ::artic::core::tree::{Handle, UiTree};
  use ::artic::controls as _;
  use ::artic::core as _;
  use ::artic::element as _;
  use ::artic::layout as _;
  use ::artic::render as _;
  use ::elements as _;
  /// The nodes of `view.xaml`: its root and one handle per named element.
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
  let rust = compile_with(
    r#"<Border xmlns="https://artic.rs/ui"/>"#,
    "views/account/login.xaml",
    |_, _| {},
  )
  .expect("a valid view");
  assert!(rust.starts_with("// Generated from views/account/login.xaml.\n"), "{rust}");
  assert!(rust.contains("/// The nodes of `views/account/login.xaml`"), "{rust}");
}

// A name two crates of one URI define is ambiguous, and the help lists both paths.
#[test]
fn name_in_two_crates_is_ambiguous() {
  let errors = compile_with(r#"<Border xmlns="https://artic.rs/ui"/>"#, "view.xaml", |map, _| {
    map.alias("https://artic.rs/ui", &[env!("CARGO_CRATE_NAME")]);
  })
  .expect_err("two Borders");
  assert!(errors.contains("`Border` is ambiguous in `https://artic.rs/ui`"), "{errors}");
  assert!(
    errors.contains("`artic_controls::border::Border`, `elements::Border`"),
    "{errors}"
  );
}

// A `using:` type is written with the module path the document gave, its crate translated
// through the crate paths.
#[test]
fn using_type_keeps_its_module_path() {
  let rust = compile(r#"<c:Border xmlns:c="using:artic_controls::border"/>"#).expect("a valid view");
  assert!(rust.contains("::artic::controls::border::Border::new()"), "{rust}");
}

// A `using:` path with no type there is unknown, naming the namespace.
#[test]
fn using_path_without_type_reports_error() {
  let errors = compile(r#"<c:Gauge xmlns:c="using:artic_controls::border"/>"#).expect_err("no Gauge");
  assert!(
    errors.contains("no type `Gauge` in `using:artic_controls::border`"),
    "{errors}"
  );
}

// A namespace that points at the app crate `with_app_crate` names, `-` read as `_`, gets the
// build-dependency help.
#[test]
fn app_crate_type_gets_build_dependency_help() {
  let xaml = Xaml::new(artic::xaml::namespaces()).with_app_crate("acme-app");
  let errors = compile_as(
    &xaml,
    r#"<l:Gauge xmlns:l="using:acme_app::views"/>"#,
    "view.xaml",
    &artic::view::crate_paths(),
  )
  .expect_err("no Gauge");
  assert!(errors.contains("a type defined in `acme_app` doesn't exist yet"), "{errors}");
}

// Without an app crate there is no build script to advise, whatever package the process runs as.
#[test]
fn no_app_crate_gives_no_build_dependency_help() {
  let package = env!("CARGO_PKG_NAME").replace('-', "_");
  let errors = compile(&format!(r#"<l:Gauge xmlns:l="using:{package}::views"/>"#)).expect_err("no Gauge");
  assert!(errors.contains("no type `Gauge`"), "{errors}");
  assert!(!errors.contains("[build-dependencies]"), "{errors}");
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

// An `x:` value provider can't be an element either.
#[test]
fn markup_value_provider_element_reports_error() {
  let errors = compile(r#"<x:Null xmlns:x="https://artic.rs/markup"/>"#).expect_err("a value provider");
  assert!(errors.contains("`x:Null` is a value provider, not an element"), "{errors}");
  assert!(errors.contains("`{x:Null …}`"), "{errors}");
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
  assert!(
    errors.contains("no type `Gauge`") && errors.contains("no type `Dial`"),
    "{errors}"
  );
}

// A crate path with generic arguments is reported, not written into code that can't parse.
#[test]
fn generic_crate_path_reports_error() {
  let errors = compile_with(r#"<Border xmlns="https://artic.rs/ui"/>"#, "view.xaml", |_, paths| {
    paths.path("artic_controls", "::artic::controls<T>");
  })
  .expect_err("not a module path");
  assert!(
    errors.contains("`::artic::controls<T>::Border` isn't a Rust path"),
    "{errors}"
  );
}

// Diagnostics come in source order, whatever order the walk finds them in.
#[test]
fn diagnostics_are_in_source_order() {
  let errors = compile(
    "<StackPanel xmlns=\"https://artic.rs/ui\">\n  <Gauge/>\n  <StackPanel.Colour>1</StackPanel.Colour>\n</StackPanel>",
  )
  .expect_err("two errors");
  let gauge = errors.find("no type `Gauge`").expect("the type error");
  let colour = errors.find("no property `Colour`").expect("the property error");
  assert!(gauge < colour, "{errors}");
}

// A URI the map doesn't declare is reported once, however many elements use it.
#[test]
fn undeclared_namespace_is_reported_once() {
  let errors = compile(r#"<Border xmlns="https://artic.rs/UI"><Border/></Border>"#).expect_err("undeclared");
  assert_eq!(errors.matches("no namespace `https://artic.rs/UI`").count(), 1, "{errors}");
  assert!(!errors.contains("no type"), "{errors}");
}

// An absolute path gives a header without a doubled separator.
#[test]
fn absolute_path_has_one_separator() {
  let rust = compile_with(r#"<Border xmlns="https://artic.rs/ui"/>"#, "/src/views/a.xaml", |_, _| {}).expect("a valid view");
  assert!(rust.starts_with("// Generated from src/views/a.xaml.\n"), "{rust}");
}

// The tree types are reached through the path for `artic_core`, so an app that renames artic can set it.
#[test]
fn tree_types_follow_the_paths() {
  let rust = compile_with(r#"<Border xmlns="https://artic.rs/ui"/>"#, "view.xaml", |_, paths| {
    paths.path("artic_core", "::ui::core");
  })
  .expect("a valid view");
  assert!(rust.contains("use ::ui::core::tree::{Handle, UiTree};"), "{rust}");
}

// The doc of the generated `View` names no syntax of the UI language.
#[test]
fn view_doc_names_no_language_syntax() {
  let rust = compile(r#"<Border xmlns="https://artic.rs/ui"/>"#).expect("a valid view");
  assert!(
    rust.contains("/// The nodes of `view.xaml`: its root and one handle per named element."),
    "{rust}"
  );
}

// Every crate the language and the paths name is referenced, sorted, so its types stay in the app's link.
#[test]
fn every_named_crate_is_linked() {
  let rust = compile(r#"<Border xmlns="https://artic.rs/ui"/>"#).expect("a valid view");
  let links: Vec<&str> = rust
    .lines()
    .filter(|line| line.starts_with("use ") && line.ends_with(" as _;") && !line.contains("Methods"))
    .collect();
  assert_eq!(
    links,
    [
      "use ::artic::controls as _;",
      "use ::artic::core as _;",
      "use ::artic::element as _;",
      "use ::artic::layout as _;",
      "use ::artic::render as _;",
      "use ::elements as _;",
    ]
  );
}

// A type whose path isn't a Rust path is reported at its name, and its members are still
// checked, since the checker runs before the path is written.
#[test]
fn unparseable_type_path_still_checks_members() {
  let errors = compile_with(r#"<Border xmlns="https://artic.rs/ui" Foo="1"/>"#, "view.xaml", |_, paths| {
    paths.path("artic_controls", "not a path");
  })
  .expect_err("a bad path");
  assert!(errors.contains("`not a path::Border` isn't a Rust path"), "{errors}");
  assert!(errors.contains("`Border` has no property `Foo`"), "{errors}");
}
