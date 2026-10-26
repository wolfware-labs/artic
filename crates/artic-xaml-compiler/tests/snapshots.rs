// The compiler's own views over artic's real types, the parser's corpus run through the compiler,
// and the properties every output has: the same text every run, and no registry in it.

use artic_diagnostics::{SourceId, SourceMap};
use artic_registry::Registry;
use artic_xaml::namespaces::NamespaceMap;
use std::path::Path;
use syn::visit::Visit;

// Each of the compiler's fixtures compiles; the snapshot is the generated Rust.
#[test]
fn fixtures() {
  insta::glob!("fixtures/*.xaml", |path| {
    let (sources, file) = load(path);
    let rust = compile(&sources, file, &artic::xaml::namespaces())
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
    let output = compile(&sources, file, &artic::xaml::namespaces()).unwrap_or_else(|errors| errors);
    insta::assert_snapshot!(output);
  });
}

// The same document gives the same text, whatever order the map was filled in.
#[test]
fn output_is_deterministic() {
  let xaml = include_str!("fixtures/user_control.xaml");
  let mut sources = SourceMap::new();
  let file = sources.add("views/login.xaml", xaml);
  let mut reversed = NamespaceMap::new();
  for (krate, path) in [
    ("artic_render", "::artic::render"),
    ("artic_layout", "::artic::layout"),
    ("artic_element", "::artic::element"),
    ("artic_controls", "::artic::controls"),
  ] {
    reversed.path(krate, path);
  }
  reversed.alias(artic::xaml::ARTIC_UI, &["artic_controls", "artic_element"]);
  let first = compile(&sources, file, &artic::xaml::namespaces()).expect("a valid view");
  let second = compile(&sources, file, &artic::xaml::namespaces()).expect("a valid view");
  let third = compile(&sources, file, &reversed).expect("a valid view");
  assert_eq!(first, second);
  assert_eq!(first, third);
}

// Generated code names no registry item, so a release app looks nothing up.
#[test]
fn compiled_output_has_no_registry() {
  insta::glob!("fixtures/*.xaml", |path| {
    let (sources, file) = load(path);
    let rust = compile(&sources, file, &artic::xaml::namespaces()).expect("a valid view");
    let mut segments = Segments(Vec::new());
    segments.visit_file(&syn::parse_file(&rust).expect("the output parses"));
    for name in ["Registry", "TypeInfo", "artic_registry", "registry"] {
      assert!(
        !segments.0.iter().any(|segment| segment == name),
        "{} names `{name}`",
        path.display()
      );
    }
  });
}

/// Every path segment in a file.
struct Segments(Vec<String>);

impl<'ast> Visit<'ast> for Segments {
  fn visit_path_segment(&mut self, segment: &'ast syn::PathSegment) {
    self.0.push(segment.ident.to_string());
    syn::visit::visit_path_segment(self, segment);
  }
}

/// Compiles `file`: the Rust, or the rendered diagnostics.
fn compile(sources: &SourceMap, file: SourceId, namespaces: &NamespaceMap) -> Result<String, String> {
  let registry = Registry::from_link_time().expect("no duplicate types");
  artic_xaml_compiler::compile_str(sources, file, &registry, namespaces).map_err(|errors| errors.render(sources))
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
