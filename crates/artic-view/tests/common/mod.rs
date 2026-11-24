//! Hand-built documents over hand-built types, as a language's front end would read them.

use artic_core::Object;
use artic_core::property::Affects;
use artic_core::tree::{NodeId, UiTree};
use artic_diagnostics::{Diagnostic, Diagnostics, SourceMap, Span};
use artic_registry::{
  AttachedPropertyInfo, AttachedTarget, ContentInfo, ContentMethod, EventInfo, PropertyAccess, PropertyInfo, PropertyRef, TypeInfo,
  ValueError,
  Whitespace,
};
use artic_view::document::{Content, Element, Member, Problem, Text, TypeUse};

/// Reads no node: the checker never builds a tree.
fn no_object(_object: &dyn Object) -> bool {
  unreachable!("the checker never reads a node")
}

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
    is: no_object,
    new: Some(new_node),
    content: None,
    properties: &[],
    groups: &[],
    events: &[],
    attached_properties: &[],
    value_provider: None,
    whitespace: Whitespace::Collapse,
    code_type_root: false,
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
    access: &UNWRITTEN,
  }
}

/// What the loader would do with a test property: nothing, since these tests never load a view.
static UNWRITTEN: PropertyAccess = PropertyAccess {
  set_text: |_, _, _, _| Err(ValueError::WrongNode),
  set_node: |_, _, _, _| Err(ValueError::WrongNode),
  set_null: None,
  clear: |_, _, _| Err(ValueError::WrongNode),
};

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
    takes: || None,
    text_wrapper: || None,
    add_child,
  }),
  properties: &PANEL_PROPERTIES,
  events: &PANEL_EVENTS,
  ..info("Panel", "test_ui::Panel")
};

/// Takes one child.
pub static FRAME: TypeInfo = TypeInfo {
  content: Some(ContentInfo::Collection {
    field: "child",
    method: Some(ContentMethod::Child),
    takes: || None,
    text_wrapper: || None,
    add_child,
  }),
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

/// Its content sets `Text`; its whitespace is significant.
pub static CODE: TypeInfo = TypeInfo {
  content: Some(ContentInfo::Property(PropertyRef {
    group: None,
    info: &TEXT[0],
  })),
  properties: &TEXT,
  whitespace: Whitespace::Significant,
  ..info("Code", "test_ui::Code")
};

/// Takes any number of children, and text as `Code` elements; its whitespace is significant.
pub static PARAGRAPH: TypeInfo = TypeInfo {
  content: Some(ContentInfo::Collection {
    field: "inlines",
    method: Some(ContentMethod::Children),
    takes: || None,
    text_wrapper: || Some(&CODE),
    add_child,
  }),
  whitespace: Whitespace::Significant,
  ..info("Paragraph", "test_ui::Paragraph")
};

/// Takes any number of children, and names `Image`, which has no content property, for its text;
/// its whitespace is significant.
pub static SHELF: TypeInfo = TypeInfo {
  content: Some(ContentInfo::Collection {
    field: "items",
    method: Some(ContentMethod::Children),
    takes: || None,
    text_wrapper: || Some(&IMAGE),
    add_child,
  }),
  whitespace: Whitespace::Significant,
  ..info("Shelf", "test_ui::Shelf")
};

/// Its content sets `Text`, but it can't be created from markup.
pub static DRAFT: TypeInfo = TypeInfo {
  new: None,
  content: Some(ContentInfo::Property(PropertyRef {
    group: None,
    info: &TEXT[0],
  })),
  properties: &TEXT,
  ..info("Draft", "test_ui::Draft")
};

/// Takes any number of children, and names `Draft`, which markup can't create, for its text.
pub static PINBOARD: TypeInfo = TypeInfo {
  content: Some(ContentInfo::Collection {
    field: "notes",
    method: Some(ContentMethod::Children),
    takes: || None,
    text_wrapper: || Some(&DRAFT),
    add_child,
  }),
  ..info("Pinboard", "test_ui::Pinboard")
};

static GALLERY_TAKES: [&TypeInfo; 1] = [&IMAGE];

/// Takes only `Image`s, and names `Label`, which it doesn't take, for its text.
pub static GALLERY: TypeInfo = TypeInfo {
  content: Some(ContentInfo::Collection {
    field: "images",
    method: Some(ContentMethod::Children),
    takes: || Some(&GALLERY_TAKES),
    text_wrapper: || Some(&LABEL),
    add_child,
  }),
  ..info("Gallery", "test_ui::Gallery")
};

static RACK_ATTACHED: [AttachedPropertyInfo; 2] = [
  AttachedPropertyInfo {
    name: "Slot",
    value_type: "usize",
    target: AttachedTarget::ChildrenOfOwner,
    literal: None,
  },
  AttachedPropertyInfo {
    name: "Tag",
    value_type: "String",
    target: AttachedTarget::Any,
    literal: None,
  },
];

/// Takes any number of children, and owns `Rack.Slot`, only for them, and `Rack.Tag`, for any
/// element.
pub static RACK: TypeInfo = TypeInfo {
  content: Some(ContentInfo::Collection {
    field: "slots",
    method: Some(ContentMethod::Children),
    takes: || None,
    text_wrapper: || None,
    add_child,
  }),
  attached_properties: &RACK_ATTACHED,
  ..info("Rack", "test_ui::Rack")
};

/// Takes one child; a view's code type can take its place at the root.
pub static HOST: TypeInfo = TypeInfo {
  content: Some(ContentInfo::Collection {
    field: "child",
    method: Some(ContentMethod::Child),
    takes: || None,
    text_wrapper: || None,
    add_child,
  }),
  code_type_root: true,
  ..info("Host", "test_ui::Host")
};

/// Its content sets `Text`; a view's code type can take its place at the root.
pub static CAPTION: TypeInfo = TypeInfo {
  content: Some(ContentInfo::Property(PropertyRef {
    group: None,
    info: &TEXT[0],
  })),
  properties: &TEXT,
  code_type_root: true,
  ..info("Caption", "test_ui::Caption")
};

/// A collection of values, which no builder method fills; a view's code type can take its place
/// at the root.
pub static BAG: TypeInfo = TypeInfo {
  content: Some(ContentInfo::Collection {
    field: "items",
    method: None,
    takes: || None,
    text_wrapper: || None,
    add_child,
  }),
  code_type_root: true,
  ..info("Bag", "test_ui::Bag")
};

/// Takes no content and has a required field, so it can't be created from markup.
pub static LOGIN: TypeInfo = TypeInfo {
  new: None,
  ..info("Login", "test_ui::Login")
};

/// Takes no content.
pub static IMAGE: TypeInfo = info("Image", "test_ui::Image");

/// A collection of values, which no builder method fills.
pub static STOPS: TypeInfo = TypeInfo {
  content: Some(ContentInfo::Collection {
    field: "stops",
    method: None,
    takes: || None,
    text_wrapper: || None,
    add_child,
  }),
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

/// The attached property `owner.name` named at `at`, set to `value` at `at + 1`.
pub fn attached(owner: &'static TypeInfo, name: &str, at: usize, value: &str) -> Member {
  Member::Attached {
    owner: Ok(TypeUse {
      info: owner,
      module: None,
      span: span(at, at + 1),
    }),
    name: name.to_owned(),
    span: span(at, at + 1),
    value: Ok(Text {
      text: value.to_owned(),
      span: span(at + 1, at + 2),
    }),
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

/// The code type `path` at `at`.
pub fn code_type(path: &str, at: usize) -> Member {
  Member::CodeType(Ok(Text {
    text: path.to_owned(),
    span: span(at, at + 1),
  }))
}

/// The field visibility `text` at `at`.
pub fn visibility(text: &str, at: usize) -> Member {
  Member::FieldVisibility(Ok(Text {
    text: text.to_owned(),
    span: span(at, at + 1),
  }))
}

/// A member the document can't hold yet, reported at `at`.
pub fn unsupported(at: usize) -> Member {
  Member::Unsupported(Diagnostic::error(span(at, at + 1), "resource keys come with #111").into())
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
