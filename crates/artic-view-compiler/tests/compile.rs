// Compiling a view of a hand-built language: the Rust it writes, the crates it links and the
// order of its diagnostics.

mod common;

use artic_diagnostics::{SourceId, SourceMap};
use artic_view::document::Document;
use artic_view_compiler::{CratePaths, compile_str};
use common::{Built, HOST, LABEL, PANEL, children, code_type, element, name, property, registry, unknown, visibility};

/// Compiles the document `build` makes, with `test_ui` reached as `::ui` and the paths `edit`
/// adds.
fn compile(build: impl Fn(SourceId) -> Document, edit: impl FnOnce(&mut CratePaths)) -> Result<String, Vec<(String, usize)>> {
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

/// `rust` without its whitespace, so a test can look for code however it's formatted.
fn squashed(rust: &str) -> String {
  rust.split_whitespace().collect()
}

/// A host whose code type is `path`, with a margin and a panel of three named labels: one
/// without a visibility, one `pub(crate)` and one `pub`.
fn login(file: SourceId, path: &str) -> Document {
  let labels = vec![
    element(file, &LABEL, 30, vec![name(file, "title", 31)]),
    element(
      file,
      &LABEL,
      40,
      vec![name(file, "subtitle", 41), visibility(file, "pub(crate)", 42)],
    ),
    element(file, &LABEL, 50, vec![name(file, "body", 51), visibility(file, "pub", 52)]),
  ];
  let panel = element(file, &PANEL, 20, vec![children(file, 29, labels)]);
  Document {
    root: element(
      file,
      &HOST,
      0,
      vec![
        code_type(file, path, 1),
        property(file, "Margin", 2, "1"),
        children(file, 19, vec![panel]),
      ],
    ),
  }
}

// A view with a code type builds its root as that type, at `crate`, from the `View` of its named
// nodes, which has no root and gives each field its visibility; `CODE_TYPE` names the type as
// written.
#[test]
fn code_type_is_the_root() {
  let rust = compile(|file| login(file, "app::views::login::Login"), |_| {}).expect("a valid view");

  syn::parse_file(&rust).expect("the output parses");
  let rust = squashed(&rust);
  for expected in [
    "#[allow(dead_code,reason=\"onlyacodetype'smacroreadsit\")]pubconstCODE_TYPE:Option<&str>=Some(\"app::views::login::Login\");",
    "pubstructView{pub(super)title:Handle<::ui::Label>,pub(crate)subtitle:Handle<::ui::Label>,pubbody:Handle<::ui::Label>,}",
    "pubfnbuild(tree:&mutUiTree)->Handle<crate::views::login::Login>{",
    "crate::views::login::Login::new(View{title:__e0,subtitle:__e1,body:__e2,}).margin(",
    ".child(__e3).build(tree)}",
  ] {
    assert!(rust.contains(expected), "{expected} in {rust}");
  }
  assert!(!rust.contains("root:"), "no root field in {rust}");
  assert!(!rust.contains("::ui::Host::new()"), "the root is the code type in {rust}");
}

// A view without a code type keeps its root in `View`, every field `pub` unless it writes
// another, and says it has no code type.
#[test]
fn view_without_a_code_type_keeps_its_root() {
  fn view(file: SourceId) -> Document {
    let label = element(
      file,
      &LABEL,
      20,
      vec![name(file, "title", 21), visibility(file, "pub(crate)", 22)],
    );
    let other = element(file, &LABEL, 30, vec![name(file, "body", 31)]);
    Document {
      root: element(file, &PANEL, 0, vec![children(file, 10, vec![label, other])]),
    }
  }

  let rust = squashed(&compile(view, |_| {}).expect("a valid view"));

  for expected in [
    "pubconstCODE_TYPE:Option<&str>=None;",
    "pubstructView{pubroot:Handle<::ui::Panel>,pub(crate)title:Handle<::ui::Label>,pubbody:Handle<::ui::Label>,}",
    "pubfnbuild(tree:&mutUiTree)->View{",
  ] {
    assert!(rust.contains(expected), "{expected} in {rust}");
  }
}

// A code type that isn't a type's full path, starting with its crate, is reported at it.
#[test]
fn code_type_must_be_a_full_path() {
  for path in [
    "Login",
    "app::views::",
    "::app::Login",
    "app::Login<T>",
    "app login",
    " app::Login",
    "app :: Login",
    "crate::views::Login",
    "app::super::Login",
    "app::self::Login",
    "app::Self",
  ] {
    let errors = compile(|file| login(file, path), |_| {}).expect_err("not a full path");

    assert_eq!(errors, [(format!("`{path}` isn't the path of a type"), 1)], "{path}");
  }
}
