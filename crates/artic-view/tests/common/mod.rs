//! Hand-built documents over hand-built types, as a language's front end would read them.

use artic_core::property::Affects;
use artic_core::tree::{NodeId, UiTree};
use artic_diagnostics::{Diagnostic, Diagnostics, SourceMap, Span};
use artic_registry::{ContentInfo, ContentMethod, EventInfo, PropertyInfo, PropertyRef, TypeInfo, ValueError, Whitespace};
use artic_view::document::{Content, Element, Member, Problem, Text, TypeUse};

/// Creates no node: the checker never builds a tree.
fn new_node(_tree: &mut UiTree) -> NodeId {
  unreachable!("the checker never creates a node")
}

/// Adds no child: the checker never builds a tree.
fn add_child(_tree: &mut UiTree, _parent: NodeId, _child: NodeId) -> Result<(), ValueError> {
  unreachable!("the checker never adds a child")
}

/// A descriptor with no content, properties or events.
const fn info(name: &'static str, rust_path: &'static str) -> TypeInfo {
  TypeInfo {
    name,
    rust_path,
    new: new_node,
    content: None,
    is_collection: false,
    properties: &[],
    groups: &[],
    events: &[],
    attached_properties: &[],
    value_provider: None,
    whitespace: Whitespace::Collapse,
  }
}

/// A text property.
const fn property_info(name: &'static str, field: &'static str) -> PropertyInfo {
  PropertyInfo {
    name,
    field,
    value_type: "String",
    affects: Affects::NONE,
    inherits: false,
    literal: None,
  }
}

static PANEL_PROPERTIES: [PropertyInfo; 1] = [property_info("Width", "width")];
static PANEL_EVENTS: [EventInfo; 1] = [EventInfo {
  name: "Click",
  args_type: "test_ui::ClickArgs",
}];
static TEXT: [PropertyInfo; 1] = [property_info("Text", "text")];

/// Takes any number of children; has `Width` and the event `Click`.
pub static PANEL: TypeInfo = TypeInfo {
  content: Some(ContentInfo::Collection {
    field: "children",
    method: Some(ContentMethod::Children),
    add_child,
  }),
  is_collection: true,
  properties: &PANEL_PROPERTIES,
  events: &PANEL_EVENTS,
  ..info("Panel", "test_ui::Panel")
};

/// Takes one child.
pub static FRAME: TypeInfo = TypeInfo {
  content: Some(ContentInfo::Collection {
    field: "child",
    method: Some(ContentMethod::Child),
    add_child,
  }),
  is_collection: true,
  ..info("Frame", "test_ui::Frame")
};

/// Its content sets `Text`, whitespace collapsed.
pub static LABEL: TypeInfo = TypeInfo {
  content: Some(ContentInfo::Property(PropertyRef {
    group: None,
    info: &TEXT[0],
  })),
  properties: &TEXT,
  ..info("Label", "test_ui::Label")
};

/// Its content sets `Text`, whitespace as written.
pub static CODE: TypeInfo = TypeInfo {
  content: Some(ContentInfo::Property(PropertyRef {
    group: None,
    info: &TEXT[0],
  })),
  properties: &TEXT,
  whitespace: Whitespace::Significant,
  ..info("Code", "test_ui::Code")
};

/// Takes no content.
pub static IMAGE: TypeInfo = info("Image", "test_ui::Image");

/// A collection of values, which no builder method fills.
pub static STOPS: TypeInfo = TypeInfo {
  content: Some(ContentInfo::Collection {
    field: "stops",
    method: None,
    add_child,
  }),
  is_collection: true,
  ..info("Stops", "test_ui::Stops")
};

/// The bytes `start..end` of the view's file.
pub fn span(start: usize, end: usize) -> Span {
  Span::new(SourceMap::new().add("view.ui", ""), start, end)
}

/// An element of type `info` written at `at`, its type's name at `at + 1`.
pub fn element(info: &'static TypeInfo, at: usize, members: Vec<Member>) -> Element {
  Element {
    ty: Ok(TypeUse {
      info,
      module: None,
      span: span(at + 1, at + 2),
    }),
    span: span(at, at + 1),
    members,
  }
}

/// An element at `at` whose type `Gauge` wasn't found, reported at `at + 1`.
pub fn unknown(at: usize, members: Vec<Member>) -> Element {
  Element {
    ty: Err(Diagnostic::error(span(at + 1, at + 2), "no type `Gauge`").into()),
    span: span(at, at + 1),
    members,
  }
}

/// An element at `at` in an undeclared namespace, a problem reported once per view.
pub fn undeclared(at: usize) -> Element {
  Element {
    ty: Err(Problem::once(
      "namespace:https://example.com/ui",
      Diagnostic::error(span(at + 1, at + 2), "no namespace `https://example.com/ui`"),
    )),
    span: span(at, at + 1),
    members: Vec::new(),
  }
}

/// The property `name` named at `at`, set to `value` at `at + 1`.
pub fn property(name: &str, at: usize, value: &str) -> Member {
  Member::Property {
    name: name.to_owned(),
    span: span(at, at + 1),
    value: Ok(Text {
      text: value.to_owned(),
      span: span(at + 1, at + 2),
    }),
  }
}

/// The property `name` named at `at`, whose value is `message`'s problem at `at + 1`.
pub fn property_problem(name: &str, at: usize, message: &str) -> Member {
  Member::Property {
    name: name.to_owned(),
    span: span(at, at + 1),
    value: Err(Diagnostic::error(span(at + 1, at + 2), message).into()),
  }
}

/// The name `text` at `at`.
pub fn name(text: &str, at: usize) -> Member {
  Member::Name(Ok(Text {
    text: text.to_owned(),
    span: span(at, at + 1),
  }))
}

/// A member the document can't hold yet, reported at `at`.
pub fn unsupported(at: usize) -> Member {
  Member::Unsupported(Diagnostic::error(span(at, at + 1), "attached properties come with #50").into())
}

/// Content starting at `at`.
pub fn content(at: usize, nodes: Vec<Content>) -> Member {
  Member::Content {
    span: span(at, at + 1),
    nodes,
  }
}

/// Text at `at`, the same after the whitespace rules as written.
pub fn text(text: &str, at: usize) -> Content {
  Content::Text {
    text: text.to_owned(),
    raw: text.to_owned(),
    span: span(at, at + 1),
  }
}

/// Indentation between elements: blank after the whitespace rules.
pub fn blank(at: usize) -> Content {
  Content::Text {
    text: String::new(),
    raw: "\n  ".to_owned(),
    span: span(at, at + 1),
  }
}

/// Each diagnostic's message and where it starts.
pub fn reported(diagnostics: &Diagnostics) -> Vec<(String, usize)> {
  diagnostics
    .iter()
    .map(|diagnostic| (diagnostic.message.clone(), diagnostic.span.start))
    .collect()
}

/// `(message, at)` as [`reported`] lists it.
pub fn at(message: &str, at: usize) -> (String, usize) {
  (message.to_owned(), at)
}
