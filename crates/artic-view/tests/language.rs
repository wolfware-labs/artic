//! A language seen through the `Language` trait.

use artic_diagnostics::{Diagnostic, Diagnostics, SourceId, SourceMap, Span};
use artic_registry::Registry;
use artic_view::{Document, Language};

/// A language that refuses every file, to check the trait's shape.
struct Refuses;

impl Language for Refuses {
  fn extension(&self) -> &str {
    "ui"
  }

  fn read(&self, _sources: &SourceMap, file: SourceId, _registry: &Registry) -> (Option<Document>, Diagnostics) {
    (None, Diagnostic::error(Span::new(file, 0, 0), "expected an element").into())
  }

  fn crates(&self) -> Vec<&str> {
    vec!["test_ui"]
  }
}

// A language can be picked at run time as a `&dyn Language`, as the CLI will by extension, and a
// file it can't read gives no document and at least one diagnostic.
#[test]
fn language_works_as_a_trait_object() {
  let mut sources = SourceMap::new();
  let file = sources.add("view.ui", "");
  let registry = Registry::from_types(&[]).expect("no types, no duplicates");
  let language: &dyn Language = &Refuses;

  let (document, diagnostics) = language.read(&sources, file, &registry);

  assert!(document.is_none());
  assert_eq!(diagnostics.len(), 1);
  assert_eq!(language.extension(), "ui");
  assert_eq!(language.crates(), ["test_ui"]);
}
