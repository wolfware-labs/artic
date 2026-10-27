//! A hand-built UI language over hand-built types, as a front end would give the compiler.

#![allow(dead_code, reason = "each test binary uses part of it")]

use artic_core::property::Affects;
use artic_core::tree::{NodeId, UiTree};
use artic_diagnostics::{Diagnostic, Diagnostics, SourceId, SourceMap, Span};
use artic_registry::{
  ContentInfo, ContentMethod, PropertyGroupInfo, PropertyInfo, Registry, RustExpr, TypeInfo, ValueError, Whitespace,
};
use artic_view::Language;
use artic_view::document::{Content, Document, Element, Member, Text, TypeUse};

/// Creates no node: the compiler never builds a tree.
fn new_node(_tree: &mut UiTree) -> NodeId {
  unreachable!("the compiler never creates a node")
}

/// Adds no child: the compiler never builds a tree.
fn add_child(_tree: &mut UiTree, _parent: NodeId, _child: NodeId) -> Result<(), ValueError> {
  unreachable!("the compiler never adds a child")
}

/// Writes a number, or refuses text that isn't one.
fn number(text: &str, out: &mut RustExpr) -> Result<(), String> {
  let value: f32 = text.parse().map_err(|_| format!("`{text}` isn't a number"))?;
  out.f32(value);
  Ok(())
}

/// Writes text as a string.
fn string(text: &str, out: &mut RustExpr) -> Result<(), String> {
  out.str(text);
  Ok(())
}

/// A descriptor with no content, properties or groups.
const fn info(name: &'static str, rust_path: &'static str) -> TypeInfo {
  TypeInfo {
    name,
    rust_path,
    new: new_node,
    content: None,
    properties: &[],
    groups: &[],
    events: &[],
    attached_properties: &[],
    value_provider: None,
    whitespace: Whitespace::Collapse,
  }
}

/// A property whose text `literal` writes.
const fn property_info(
  name: &'static str,
  field: &'static str,
  literal: fn(&str, &mut RustExpr) -> Result<(), String>,
) -> PropertyInfo {
  PropertyInfo {
    name,
    field,
    value_type: "f32",
    affects: Affects::NONE,
    inherits: false,
    literal: Some(literal),
  }
}

static WIDTH: [PropertyInfo; 1] = [property_info("Width", "width", number)];
static MARGIN: [PropertyInfo; 1] = [property_info("Margin", "margin", number)];
static LAYOUT: [PropertyGroupInfo; 1] = [PropertyGroupInfo {
  field: "layout",
  properties: &MARGIN,
  methods: "test_ui::layout::LayoutMethods",
}];
static TEXT: [PropertyInfo; 1] = [property_info("Text", "text", string)];

/// Takes any number of children; has `Width` and the group `layout` with `Margin`.
pub static PANEL: TypeInfo = TypeInfo {
  content: Some(ContentInfo::Collection {
    field: "children",
    method: Some(ContentMethod::Children),
    add_child,
  }),
  properties: &WIDTH,
  groups: &LAYOUT,
  ..info("Panel", "test_ui::panel::Panel")
};

/// Has `Text`.
pub static LABEL: TypeInfo = TypeInfo {
  properties: &TEXT,
  ..info("Label", "test_ui::label::Label")
};

/// The registry of the hand-built types.
pub fn registry() -> Registry {
  Registry::from_types(&[&PANEL, &LABEL]).expect("no duplicate types")
}

/// The language's crates.
const CRATES: [&str; 1] = ["test_ui"];

/// A language whose reader returns the document `build` makes for the file.
pub struct Built(pub fn(SourceId) -> Document);

impl Language for Built {
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

/// The bytes `at..at + 1` of `file`.
pub fn span(file: SourceId, at: usize) -> Span {
  Span::new(file, at, at + 1)
}

/// An element of type `info` written at `at`.
pub fn element(file: SourceId, info: &'static TypeInfo, at: usize, members: Vec<Member>) -> Element {
  Element {
    ty: Ok(TypeUse {
      info,
      module: None,
      span: span(file, at),
    }),
    span: span(file, at),
    members,
  }
}

/// An element at `at` whose type `Gauge` wasn't found.
pub fn unknown(file: SourceId, at: usize) -> Element {
  Element {
    ty: Err(Diagnostic::error(span(file, at), "no type `Gauge`").into()),
    span: span(file, at),
    members: Vec::new(),
  }
}

/// The property `name` set to `value`, both written at `at`.
pub fn property(file: SourceId, name: &str, at: usize, value: &str) -> Member {
  Member::Property {
    name: name.to_owned(),
    span: span(file, at),
    value: Ok(Text {
      text: value.to_owned(),
      span: span(file, at),
    }),
  }
}

/// The name `text` at `at`.
pub fn name(file: SourceId, text: &str, at: usize) -> Member {
  Member::Name(Ok(Text {
    text: text.to_owned(),
    span: span(file, at),
  }))
}

/// Content of `elements`, starting at `at`.
pub fn children(file: SourceId, at: usize, elements: Vec<Element>) -> Member {
  Member::Content {
    span: span(file, at),
    nodes: elements.into_iter().map(Content::Element).collect(),
  }
}
