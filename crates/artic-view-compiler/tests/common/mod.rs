//! A hand-built UI language over hand-built types, as a front end would give the compiler.

#![allow(dead_code, reason = "each test binary uses part of it")]

use artic_diagnostics::{Diagnostic, Diagnostics, SourceId, SourceMap, Span};
use artic_registry::{Registry, TypeInfo};
use artic_view::Language;
use artic_view::document::{Content, Document, Element, Member, TypeUse};
use artic_view::testing::{HOST, LABEL, PANEL, info, span};

/// Takes no content; a code type can take its place.
pub static BADGE: TypeInfo = info("Badge", "test_ui::badge::Badge").with_code_type_root(true);

/// The registry of the hand-built types.
pub fn registry() -> Registry {
  Registry::from_types(&[&PANEL, &LABEL, &HOST, &BADGE]).expect("no duplicate types")
}

/// The language's crates.
const CRATES: [&str; 1] = ["test_ui"];

/// A language whose reader returns the document `build` makes for the file.
pub struct Built<F>(pub F);

impl<F> Language for Built<F>
where
  F: Fn(SourceId) -> Document,
{
  fn extension(&self) -> &str {
    "view"
  }

  fn read(&self, _sources: &SourceMap, file: SourceId, _registry: &Registry) -> (Option<Document>, Diagnostics) {
    (Some(self.0(file)), Diagnostics::new())
  }

  fn crates(&self) -> Vec<&str> {
    CRATES.to_vec()
  }
}

/// A language whose file is the name of its root's type, such as `Panel`.
pub struct Named;

impl Language for Named {
  fn extension(&self) -> &str {
    "view"
  }

  fn read(&self, sources: &SourceMap, file: SourceId, registry: &Registry) -> (Option<Document>, Diagnostics) {
    let name = sources.get(file).text().trim();
    let at = Span::new(file, 0, name.len());
    let ty = registry
      .type_by_name("test_ui", name)
      .found()
      .map(|info| TypeUse {
        info,
        module: None,
        span: at,
      })
      .ok_or_else(|| Diagnostic::error(at, format!("no type `{name}`")).into());
    let root = Element {
      ty,
      span: at,
      members: Vec::new(),
    };
    (Some(Document { root }), Diagnostics::new())
  }

  fn crates(&self) -> Vec<&str> {
    CRATES.to_vec()
  }
}

/// A language that reads no document and reports nothing, against `Language::read`'s contract.
pub struct Silent;

impl Language for Silent {
  fn extension(&self) -> &str {
    "view"
  }

  fn read(&self, _sources: &SourceMap, _file: SourceId, _registry: &Registry) -> (Option<Document>, Diagnostics) {
    (None, Diagnostics::new())
  }

  fn crates(&self) -> Vec<&str> {
    Vec::new()
  }
}

/// Content of `elements`, starting at `at`.
pub fn children(file: SourceId, at: usize, elements: Vec<Element>) -> Member {
  Member::Content {
    span: span(file, at, at + 1),
    nodes: elements.into_iter().map(Content::Element).collect(),
  }
}
