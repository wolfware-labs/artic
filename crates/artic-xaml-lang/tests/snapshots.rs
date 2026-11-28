// The compiler's own views over artic's real types, the parser's corpus run through the compiler,
// and the properties every output has: the same text every run, and no registry or loader access
// in it.

use artic::view::CratePaths;
use artic_diagnostics::{SourceId, SourceMap};
use artic_registry::Registry;
use artic_xaml_lang::Xaml;
use proc_macro2::{TokenStream, TokenTree};
use std::path::Path;
use syn::visit::Visit;

// Each of the compiler's fixtures compiles; the snapshot is the generated Rust.
#[test]
fn fixtures() {
  insta::glob!("fixtures/*.xaml", |path| {
    let (sources, file) = load(path);
    let rust = compile(&sources, file, &artic::view::crate_paths())
      .unwrap_or_else(|errors| panic!("{} should compile:\n{errors}", path.display()));
    insta::assert_snapshot!(rust);
  });
}

// Every valid fixture of the parser's corpus compiles or reports diagnostics, never panics; the
// snapshot is the Rust or the rendered diagnostics.
#[test]
fn parser_corpus() {
  insta::glob!("../../artic-xaml/tests/fixtures", "valid/**/*.xaml", |path| {
    let (sources, file) = load(path);
    let output = compile(&sources, file, &artic::view::crate_paths()).unwrap_or_else(|errors| errors);
    insta::assert_snapshot!(output);
  });
}

// The same document gives the same text, whatever order the map and the paths were filled in.
#[test]
fn output_is_deterministic() {
  let xaml = include_str!("fixtures/user_control.xaml");
  let mut sources = SourceMap::new();
  let file = sources.add("views/login.xaml", xaml);
  let mut reversed = CratePaths::new();
  for (krate, path) in [
    ("artic_render", "::artic::render"),
    ("artic_layout", "::artic::layout"),
    ("artic_element", "::artic::element"),
    ("artic_controls", "::artic::controls"),
    ("artic_core", "::artic::core"),
  ] {
    reversed.path(krate, path);
  }
  let mut namespaces = artic::xaml::NamespaceMap::new();
  namespaces.alias(artic::xaml::ARTIC_UI, &["artic_controls", "artic_element"]);
  let first = compile(&sources, file, &artic::view::crate_paths()).expect("a valid view");
  let second = compile(&sources, file, &artic::view::crate_paths()).expect("a valid view");
  let third = compile_with(&sources, file, namespaces, &reversed).expect("a valid view");
  assert_eq!(first, second);
  assert_eq!(first, third);
}

/// The registry's names, which a release app never looks anything up through.
const REGISTRY_NAMES: [&str; 4] = ["Registry", "TypeInfo", "artic_registry", "registry"];

/// The names of a runtime loader's way in: a property's access, its thunks, a collection's
/// `add_child` and the error they return.
const LOADER_NAMES: [&str; 8] = [
  "PropertyAccess",
  "access",
  "set_text",
  "set_node",
  "set_object",
  "set_null",
  "add_child",
  "ValueError",
];

// Generated code names no registry item, so a release app looks nothing up.
#[test]
fn compiled_output_has_no_registry() {
  insta::glob!("fixtures/*.xaml", |path| {
    let (sources, file) = load(path);
    let rust = compile(&sources, file, &artic::view::crate_paths()).expect("a valid view");
    assert_eq!(named(&rust, &REGISTRY_NAMES), Vec::<String>::new(), "{}", path.display());
  });
}

// Generated code never writes a property through the loader's access: it calls typed builder
// methods, for the compiler's fixtures and every view of the parser's corpus that compiles.
#[test]
fn compiled_output_has_no_loader_values() {
  let check = |path: &Path| {
    let (sources, file) = load(path);
    if let Ok(rust) = compile(&sources, file, &artic::view::crate_paths()) {
      assert_eq!(named(&rust, &LOADER_NAMES), Vec::<String>::new(), "{}", path.display());
    }
  };
  insta::glob!("fixtures/*.xaml", check);
  insta::glob!("../../artic-xaml/tests/fixtures", "valid/**/*.xaml", check);
}

// The guards see each of their names in a path, a method call, a field or a macro's tokens, and
// not in a string or a comment.
#[test]
fn guards_see_names_in_code_only() {
  let loader = "fn f(t: T) { let _: PropertyAccess = t.access; ::m::set_text(); t.set_node(1); \
    t.set_object(); t.set_null(); t.add_child(); let _: ValueError = e; }";
  let registry = "fn f(t: T) { ::m::Registry::new(); let _: TypeInfo = t.x; ::artic_registry::f(); t.registry; }";
  let in_macro = "fn f() { m!(f(x.set_text())); m![{ y.access }]; }";
  let quoted = "// set_text\nfn f() { let _ = \"set_text access\"; m!(\"set_text\"); }";

  assert_eq!(
    named(loader, &LOADER_NAMES),
    [
      "PropertyAccess",
      "access",
      "set_text",
      "set_node",
      "set_object",
      "set_null",
      "add_child",
      "ValueError"
    ]
  );
  assert_eq!(
    named(registry, &REGISTRY_NAMES),
    ["Registry", "TypeInfo", "artic_registry", "registry"]
  );
  assert_eq!(named(in_macro, &LOADER_NAMES), ["set_text", "access"]);
  assert_eq!(named(quoted, &LOADER_NAMES), Vec::<String>::new());
}

/// The names among `forbidden` that the Rust file `rust` uses as identifiers, in order.
fn named(rust: &str, forbidden: &[&str]) -> Vec<String> {
  let mut idents = Idents(Vec::new());
  idents.visit_file(&syn::parse_file(rust).expect("the output parses"));
  idents
    .0
    .into_iter()
    .filter(|name| forbidden.contains(&name.as_str()))
    .collect()
}

/// Every identifier in a file: path segments, method names, fields and bindings alike, and those
/// in a macro call's tokens.
struct Idents(Vec<String>);

impl Idents {
  /// Collects every identifier in `tokens`, inside groups too.
  fn tokens(&mut self, tokens: TokenStream) {
    for tree in tokens {
      match tree {
        TokenTree::Ident(ident) => self.0.push(ident.to_string()),
        TokenTree::Group(group) => self.tokens(group.stream()),
        TokenTree::Punct(_) | TokenTree::Literal(_) => {}
      }
    }
  }
}

impl<'ast> Visit<'ast> for Idents {
  fn visit_ident(&mut self, ident: &'ast syn::Ident) {
    self.0.push(ident.to_string());
  }

  fn visit_macro(&mut self, mac: &'ast syn::Macro) {
    syn::visit::visit_macro(self, mac);
    self.tokens(mac.tokens.clone());
  }
}

/// Compiles `file` with artic's map and `paths`: the Rust, or the rendered diagnostics.
fn compile(sources: &SourceMap, file: SourceId, paths: &CratePaths) -> Result<String, String> {
  compile_with(sources, file, artic::xaml::namespaces(), paths)
}

/// Compiles `file` with `namespaces` and `paths`: the Rust, or the rendered diagnostics.
fn compile_with(
  sources: &SourceMap,
  file: SourceId,
  namespaces: artic::xaml::NamespaceMap,
  paths: &CratePaths,
) -> Result<String, String> {
  let registry = Registry::from_link_time().expect("no duplicate types");
  artic_view_compiler::compile_str(sources, file, &Xaml::new(namespaces), &registry, paths)
    .map_err(|errors| errors.render(sources))
}

/// Loads a fixture under the path it has inside `fixtures/`, with `/` separators, so the output
/// reads the same on every machine.
fn load(path: &Path) -> (SourceMap, SourceId) {
  let parts: Vec<_> = path.iter().map(|part| part.to_string_lossy()).collect();
  let fixtures = parts
    .iter()
    .rposition(|part| part == "fixtures")
    .expect("a fixture is under a fixtures directory");
  let text = std::fs::read_to_string(path).expect("a readable fixture");
  let mut sources = SourceMap::new();
  let file = sources.add(parts[fixtures + 1..].join("/"), text);
  (sources, file)
}
