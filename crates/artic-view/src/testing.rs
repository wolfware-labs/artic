//! Hand-built types and documents for tests, as a language's front end would read them: types no
//! node backs, and members written at given byte offsets of a file.

use crate::document::{Element, Member, Text, TypeUse};
use artic_core::Object;
use artic_core::tree::{NodeId, UiTree};
use artic_diagnostics::{Diagnostic, SourceId, Span};
use artic_registry::{
  ContentInfo, ContentMethod, EventInfo, PropertyGroupInfo, PropertyInfo, PropertyRef, RustExpr, TypeInfo, ValueError,
};

/// Reads no node: these tests never build a tree.
pub fn no_object(_object: &dyn Object) -> bool {
  unreachable!("these tests never read a node")
}

/// Creates no node: these tests never build a tree.
pub fn new_node(_tree: &mut UiTree) -> NodeId {
  unreachable!("these tests never create a node")
}

/// Adds no child: these tests never build a tree.
pub fn add_child(_tree: &mut UiTree, _parent: NodeId, _child: NodeId) -> Result<(), ValueError> {
  unreachable!("these tests never add a child")
}

/// A node type named `name` at `rust_path` with no content, properties, groups or events, which
/// markup can create.
pub const fn info(name: &'static str, rust_path: &'static str) -> TypeInfo {
  TypeInfo::node(name, rust_path, no_object).with_new(new_node)
}

/// Writes a number, or refuses text that isn't one.
pub fn number(text: &str, out: &mut RustExpr) -> Result<(), String> {
  let value: f32 = text.parse().map_err(|_| format!("`{text}` isn't a number"))?;
  out.f32(value);
  Ok(())
}

/// Writes text as a string.
pub fn string(text: &str, out: &mut RustExpr) -> Result<(), String> {
  out.str(text);
  Ok(())
}

static WIDTH: [PropertyInfo; 1] = [PropertyInfo::new("Width", "width", "f32").with_literal(number)];
static MARGIN: [PropertyInfo; 1] = [PropertyInfo::new("Margin", "margin", "f32").with_literal(number)];
static LAYOUT: [PropertyGroupInfo; 1] = [PropertyGroupInfo {
  field: "layout",
  properties: &MARGIN,
  methods: "test_ui::layout::LayoutMethods",
}];
static PANEL_EVENTS: [EventInfo; 1] = [EventInfo {
  name: "Click",
  args_type: "test_ui::ClickArgs",
}];

/// The property `Text`, written as a string.
pub static TEXT: [PropertyInfo; 1] = [PropertyInfo::new("Text", "text", "String").with_literal(string)];

/// Takes any number of children; has `Width`, the group `layout` with `Margin`, and the event
/// `Click`.
pub static PANEL: TypeInfo = info("Panel", "test_ui::panel::Panel")
  .with_content(ContentInfo::Collection {
    field: "children",
    method: Some(ContentMethod::Children),
    takes: || None,
    text_wrapper: || None,
    add_child,
  })
  .with_properties(&WIDTH)
  .with_groups(&LAYOUT)
  .with_events(&PANEL_EVENTS);

/// Takes one child; has the group `layout` with `Margin`; a view's code type can take its place
/// at the root.
pub static HOST: TypeInfo = info("Host", "test_ui::host::Host")
  .with_content(ContentInfo::Collection {
    field: "child",
    method: Some(ContentMethod::Child),
    takes: || None,
    text_wrapper: || None,
    add_child,
  })
  .with_groups(&LAYOUT)
  .with_code_type_root(true);

/// Its content sets `Text`, whitespace collapsed.
pub static LABEL: TypeInfo = info("Label", "test_ui::label::Label")
  .with_content(ContentInfo::Property(PropertyRef {
    group: None,
    info: &TEXT[0],
  }))
  .with_properties(&TEXT);

/// The bytes `start..end` of `file`.
pub fn span(file: SourceId, start: usize, end: usize) -> Span {
  Span::new(file, start, end)
}

/// An element of type `info` written at `at`, its type's name at `at + 1`.
pub fn element(file: SourceId, info: &'static TypeInfo, at: usize, members: Vec<Member>) -> Element {
  Element {
    ty: Ok(TypeUse {
      info,
      module: None,
      span: span(file, at + 1, at + 2),
    }),
    span: span(file, at, at + 1),
    members,
  }
}

/// An element at `at` whose type `Gauge` wasn't found, reported at `at + 1`.
pub fn unknown(file: SourceId, at: usize, members: Vec<Member>) -> Element {
  Element {
    ty: Err(Diagnostic::error(span(file, at + 1, at + 2), "no type `Gauge`").into()),
    span: span(file, at, at + 1),
    members,
  }
}

/// The property `name` named at `at`, set to `value` at `at + 1`.
pub fn property(file: SourceId, name: &str, at: usize, value: &str) -> Member {
  Member::Property {
    name: name.to_owned(),
    span: span(file, at, at + 1),
    value: Ok(Text {
      text: value.to_owned(),
      span: span(file, at + 1, at + 2),
    }),
  }
}

/// The name `text` at `at`.
pub fn name(file: SourceId, text: &str, at: usize) -> Member {
  Member::Name(Ok(Text {
    text: text.to_owned(),
    span: span(file, at, at + 1),
  }))
}

/// The code type `path` at `at`.
pub fn code_type(file: SourceId, path: &str, at: usize) -> Member {
  Member::CodeType(Ok(Text {
    text: path.to_owned(),
    span: span(file, at, at + 1),
  }))
}

/// The field visibility `text` at `at`.
pub fn visibility(file: SourceId, text: &str, at: usize) -> Member {
  Member::FieldVisibility(Ok(Text {
    text: text.to_owned(),
    span: span(file, at, at + 1),
  }))
}
