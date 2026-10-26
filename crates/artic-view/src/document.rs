//! A view as its language wrote it: types looked up, members not yet checked.
//!
//! A front end builds it, and so can a test. `Element` here is the document's element; code that
//! also uses `artic_element::Element` names this one through the module.

use artic_diagnostics::{Diagnostic, Span};
use artic_registry::TypeInfo;

/// A view as its language wrote it: types looked up, members not yet checked.
#[derive(Debug, Clone)]
pub struct Document {
  /// The view's root element.
  pub root: Element,
}

/// An element of the view.
#[derive(Debug, Clone)]
pub struct Element {
  /// The element's type, or what the checker reports if it reaches the element.
  pub ty: Result<TypeUse, Problem>,
  /// Where the element is written.
  pub span: Span,
  /// Its members, in the language's member order, such as attributes, then properties written
  /// as elements, then the content.
  pub members: Vec<Member>,
}

/// A type an element uses, and where the document reached it.
#[derive(Debug, Clone)]
pub struct TypeUse {
  /// The type's descriptor.
  pub info: &'static TypeInfo,
  /// The module the document named the type in, such as `artic_controls::border`, or `None` when
  /// the type is reached at its crate's root.
  pub module: Option<String>,
  /// Where the type's name is written.
  pub span: Span,
}

/// One member of an element.
#[derive(Debug, Clone)]
pub enum Member {
  /// A property set to text.
  Property {
    /// The property's name as written.
    name: String,
    /// Where the member is named: an unknown property is reported here.
    span: Span,
    /// The value, or what the checker reports once it finds the property.
    value: Result<Text, Problem>,
  },
  /// The name the view's code reaches the element by.
  Name(Result<Text, Problem>),
  /// What is written directly inside the element, every node kept, blank text included.
  Content {
    /// From the first node to the last: the span a content property's text is set at.
    span: Span,
    /// The nodes, in order.
    nodes: Vec<Content>,
  },
  /// A member the document can't hold yet, reported whenever the checker reaches the element.
  Unsupported(Problem),
}

/// Something written directly inside an element.
#[derive(Debug, Clone)]
pub enum Content {
  /// Text.
  Text {
    /// The text after the language's whitespace rules; empty for blank text.
    text: String,
    /// The text as written, which a type whose whitespace is significant reads.
    raw: String,
    /// Where it is written.
    span: Span,
  },
  /// A child element.
  Element(Element),
  /// A node the document can't hold yet, reported if the checker reads the content as text.
  Unsupported(Problem),
}

/// Text with the span it was written at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
  /// The text.
  pub text: String,
  /// Where it is written.
  pub span: Span,
}

/// A problem a front end found, which the checker reports if it reaches it.
#[derive(Debug, Clone)]
pub struct Problem {
  /// What to report.
  pub diagnostic: Box<Diagnostic>,
  /// A key reported only once per view, at the first use the checker reaches, prefixed by its
  /// kind, such as `namespace:` and the URI of an undeclared namespace.
  pub once: Option<String>,
}

impl Problem {
  /// A problem reported wherever the checker reaches it.
  #[must_use]
  pub fn new(diagnostic: Diagnostic) -> Problem {
    Problem {
      diagnostic: Box::new(diagnostic),
      once: None,
    }
  }

  /// A problem reported only at the first use of `key` the checker reaches.
  #[must_use]
  pub fn once(key: impl Into<String>, diagnostic: Diagnostic) -> Problem {
    Problem {
      diagnostic: Box::new(diagnostic),
      once: Some(key.into()),
    }
  }
}

impl From<Diagnostic> for Problem {
  fn from(diagnostic: Diagnostic) -> Problem {
    Problem::new(diagnostic)
  }
}
