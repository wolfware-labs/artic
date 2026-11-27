//! Hand-built documents over hand-built types, as a language's front end would read them.

use artic_diagnostics::{Diagnostic, Diagnostics, SourceId, SourceMap};
use artic_registry::{
  AttachedPropertyInfo, AttachedTarget, ContentInfo, ContentMethod, PropertyRef, TypeInfo, TypeKind, Whitespace,
};
use artic_view::document::{Content, Element, Member, Problem, Text, TypeUse};
use artic_view::testing::{LABEL, TEXT, add_child, info, no_object, span};

/// The view's file, the one source these tests make.
pub fn file() -> SourceId {
  SourceMap::new().add("view.ui", "")
}

/// Takes one child.
pub static FRAME: TypeInfo = info("Frame", "test_ui::Frame").with_content(ContentInfo::Collection {
  field: "child",
  method: Some(ContentMethod::Child),
  takes: || None,
  text_wrapper: || None,
  add_child,
});

/// Its content sets `Text`; its whitespace is significant.
pub static CODE: TypeInfo = info("Code", "test_ui::Code")
  .with_content(ContentInfo::Property(PropertyRef {
    group: None,
    info: &TEXT[0],
  }))
  .with_properties(&TEXT)
  .with_whitespace(Whitespace::Significant);

/// Takes any number of children, and text as `Code` elements; its whitespace is significant.
pub static PARAGRAPH: TypeInfo = info("Paragraph", "test_ui::Paragraph")
  .with_content(ContentInfo::Collection {
    field: "inlines",
    method: Some(ContentMethod::Children),
    takes: || None,
    text_wrapper: || Some(&CODE),
    add_child,
  })
  .with_whitespace(Whitespace::Significant);

/// Takes any number of children, and names `Image`, which has no content property, for its text;
/// its whitespace is significant.
pub static SHELF: TypeInfo = info("Shelf", "test_ui::Shelf")
  .with_content(ContentInfo::Collection {
    field: "items",
    method: Some(ContentMethod::Children),
    takes: || None,
    text_wrapper: || Some(&IMAGE),
    add_child,
  })
  .with_whitespace(Whitespace::Significant);

/// Its content sets `Text`, but it can't be created from markup.
pub static DRAFT: TypeInfo = TypeInfo::node("Draft", "test_ui::Draft", no_object)
  .with_content(ContentInfo::Property(PropertyRef {
    group: None,
    info: &TEXT[0],
  }))
  .with_properties(&TEXT);

/// Takes any number of children, and names `Draft`, which markup can't create, for its text.
pub static PINBOARD: TypeInfo = info("Pinboard", "test_ui::Pinboard").with_content(ContentInfo::Collection {
  field: "notes",
  method: Some(ContentMethod::Children),
  takes: || None,
  text_wrapper: || Some(&DRAFT),
  add_child,
});

static GALLERY_TAKES: [&TypeInfo; 1] = [&IMAGE];

/// Takes only `Image`s, and names `Label`, which it doesn't take, for its text.
pub static GALLERY: TypeInfo = info("Gallery", "test_ui::Gallery").with_content(ContentInfo::Collection {
  field: "images",
  method: Some(ContentMethod::Children),
  takes: || Some(&GALLERY_TAKES),
  text_wrapper: || Some(&LABEL),
  add_child,
});

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
pub static RACK: TypeInfo = info("Rack", "test_ui::Rack")
  .with_content(ContentInfo::Collection {
    field: "slots",
    method: Some(ContentMethod::Children),
    takes: || None,
    text_wrapper: || None,
    add_child,
  })
  .with_attached_properties(&RACK_ATTACHED);

/// Its content sets `Text`; a view's code type can take its place at the root.
pub static CAPTION: TypeInfo = info("Caption", "test_ui::Caption")
  .with_content(ContentInfo::Property(PropertyRef {
    group: None,
    info: &TEXT[0],
  }))
  .with_properties(&TEXT)
  .with_code_type_root(true);

/// A collection of values, which no builder method fills; a view's code type can take its place
/// at the root.
pub static BAG: TypeInfo = info("Bag", "test_ui::Bag")
  .with_content(ContentInfo::Collection {
    field: "items",
    method: None,
    takes: || None,
    text_wrapper: || None,
    add_child,
  })
  .with_code_type_root(true);

/// Takes no content and has a required field, so it can't be created from markup.
pub static LOGIN: TypeInfo = TypeInfo::node("Login", "test_ui::Login", no_object);

/// A value: staged only while its members are set, never a node of the tree.
pub static TINT: TypeInfo = info("Tint", "test_ui::Tint").with_kind(TypeKind::Value);

/// Takes no content.
pub static IMAGE: TypeInfo = info("Image", "test_ui::Image");

/// A collection of values, which no builder method fills.
pub static STOPS: TypeInfo = info("Stops", "test_ui::Stops").with_content(ContentInfo::Collection {
  field: "stops",
  method: None,
  takes: || None,
  text_wrapper: || None,
  add_child,
});

/// An element at `at` in an undeclared namespace, a problem reported once per view.
pub fn undeclared(file: SourceId, at: usize) -> Element {
  Element {
    ty: Err(Problem::once(
      "namespace:https://example.com/ui",
      Diagnostic::error(span(file, at + 1, at + 2), "no namespace `https://example.com/ui`"),
    )),
    span: span(file, at, at + 1),
    members: Vec::new(),
  }
}

/// The attached property `owner.name` named at `at`, set to `value` at `at + 1`.
pub fn attached(file: SourceId, owner: &'static TypeInfo, name: &str, at: usize, value: &str) -> Member {
  Member::Attached {
    owner: Ok(TypeUse {
      info: owner,
      module: None,
      span: span(file, at, at + 1),
    }),
    name: name.to_owned(),
    span: span(file, at, at + 1),
    value: Ok(Text {
      text: value.to_owned(),
      span: span(file, at + 1, at + 2),
    }),
  }
}

/// The property `name` named at `at`, whose value is `message`'s problem at `at + 1`.
pub fn property_problem(file: SourceId, name: &str, at: usize, message: &str) -> Member {
  Member::Property {
    name: name.to_owned(),
    span: span(file, at, at + 1),
    value: Err(Diagnostic::error(span(file, at + 1, at + 2), message).into()),
  }
}

/// A member the document can't hold yet, reported at `at`.
pub fn unsupported(file: SourceId, at: usize) -> Member {
  Member::Unsupported(Diagnostic::error(span(file, at, at + 1), "resource keys come with #111").into())
}

/// Content starting at `at`.
pub fn content(file: SourceId, at: usize, nodes: Vec<Content>) -> Member {
  Member::Content {
    span: span(file, at, at + 1),
    nodes,
  }
}

/// Text at `at`, the same after the whitespace rules as written.
pub fn text(file: SourceId, text: &str, at: usize) -> Content {
  Content::Text {
    text: text.to_owned(),
    raw: text.to_owned(),
    span: span(file, at, at + 1),
  }
}

/// Indentation between elements: blank after the whitespace rules.
pub fn blank(file: SourceId, at: usize) -> Content {
  Content::Text {
    text: String::new(),
    raw: "\n  ".to_owned(),
    span: span(file, at, at + 1),
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
