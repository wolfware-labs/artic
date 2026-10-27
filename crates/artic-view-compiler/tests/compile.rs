// Compiling a view of a hand-built language: the Rust it writes, the crates it links and the
// order of its diagnostics.

mod common;

use artic_diagnostics::{SourceId, SourceMap};
use artic_view::document::Document;
use artic_view_compiler::{CratePaths, compile_str};
use common::{Built, LABEL, PANEL, children, element, name, property, registry, unknown};

/// Compiles the document `build` makes, with `test_ui` reached as `::ui` and the paths `edit`
/// adds.
fn compile(build: fn(SourceId) -> Document, edit: impl FnOnce(&mut CratePaths)) -> Result<String, Vec<(String, usize)>> {
  let mut sources = SourceMap::new();
  let file = sources.add("view.view", "");
  let mut paths = CratePaths::new();
  paths.path("test_ui", "::ui");
  edit(&mut paths);
  compile_str(&sources, file, &Built(build), &registry(), &paths).map_err(|diagnostics| {
    diagnostics
      .iter()
      .map(|diagnostic| (diagnostic.message.clone(), diagnostic.span.start))
      .collect()
  })
}

/// A panel with a width, a margin and one named label.
fn form(file: SourceId) -> Document {
  let title = element(
    file,
    &LABEL,
    20,
    vec![name(file, "title", 21), property(file, "Text", 22, "Hi")],
  );
  Document {
    root: element(
      file,
      &PANEL,
      0,
      vec![
        property(file, "Width", 1, "2"),
        property(file, "Margin", 2, "1"),
        children(file, 10, vec![title]),
      ],
    ),
  }
}

// A view of a hand-built language compiles to Rust that parses, names each type
// through the crate's path, imports the group's methods and links the language's crates.
#[test]
fn hand_built_view_compiles() {
  let rust = compile(form, |_| {}).expect("a valid view");

  syn::parse_file(&rust).expect("the output parses");
  for expected in [
    "use ::artic_core::tree::{Handle, UiTree};",
    "use ::ui::LayoutMethods as _;",
    "use ::ui as _;",
    "::ui::Panel::new()",
    "pub title: Handle<::ui::Label>",
  ] {
    assert!(rust.contains(expected), "{expected} in {rust}");
  }
}

// A literal emission refuses and a type the checker reports come back in source order.
#[test]
fn diagnostics_are_sorted_across_stages() {
  fn view(file: SourceId) -> Document {
    Document {
      root: element(
        file,
        &PANEL,
        0,
        vec![property(file, "Width", 3, "wide"), children(file, 9, vec![unknown(file, 10)])],
      ),
    }
  }

  let errors = compile(view, |_| {}).expect_err("two problems");

  assert_eq!(
    errors,
    [("`wide` isn't a number".to_owned(), 3), ("no type `Gauge`".to_owned(), 10)]
  );
}

// A crate path that isn't a Rust path is reported, pointing at the paths that were set.
#[test]
fn unparseable_path_names_crate_paths() {
  let mut sources = SourceMap::new();
  let file = sources.add("view.view", "");
  let mut paths = CratePaths::new();
  paths.path("test_ui", "not a path");

  let diagnostics = compile_str(&sources, file, &Built(form), &registry(), &paths).expect_err("a bad path");

  let diagnostic = diagnostics.iter().next().expect("a diagnostic");
  assert!(diagnostic.message.contains("isn't a Rust path"), "{}", diagnostic.message);
  assert_eq!(
    diagnostic.help.as_deref(),
    Some("check the paths set with `CratePaths::path`")
  );
}

// The links are the language's crates and every crate given a path, each once, by crate name.
#[test]
fn links_are_the_language_crates_and_the_paths_once_sorted() {
  let rust = compile(form, |paths| {
    paths.path("zed", "::zed_path");
    paths.path("alpha", "::alpha");
  })
  .expect("a valid view");

  let links: Vec<&str> = rust
    .lines()
    .filter(|line| line.starts_with("use ") && line.ends_with(" as _;") && !line.contains("Methods"))
    .collect();
  assert_eq!(links, ["use ::alpha as _;", "use ::ui as _;", "use ::zed_path as _;"]);
}
