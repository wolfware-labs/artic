//! Checking a document's members against its types.

use crate::Document;
use crate::document::{Text, TypeUse};
use artic_diagnostics::{Diagnostics, Span};
use artic_registry::PropertyRef;

/// A view whose members are all found, ready for a back end.
#[derive(Debug, Clone)]
pub struct CheckedView {
  /// Every element whose type was found, children before their parent.
  pub elements: Vec<CheckedElement>,
  /// The root's index, or `None` when its type wasn't found.
  pub root: Option<usize>,
  /// Where the root element is written, for problems with the whole file.
  pub span: Span,
  /// Each valid name with its element's index, in the order the elements are built.
  pub names: Vec<(Text, usize)>,
}

/// An element ready to build: its type, its properties and its children.
#[derive(Debug, Clone)]
pub struct CheckedElement {
  /// The element's type.
  pub ty: TypeUse,
  /// Each property with the text it's set to, in member order.
  pub properties: Vec<(PropertyRef, Text)>,
  /// The children its content method takes.
  pub content: CheckedContent,
}

/// The children an element's content method takes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CheckedContent {
  /// No content call.
  #[default]
  None,
  /// `child`, with the child's index.
  Child(usize),
  /// `children`, with the children's indices in document order; never empty.
  Children(Vec<usize>),
}

/// Checks `document`'s members: each property against its type's descriptor, the content against
/// the type's content rule, and the names. Returns the view and every problem the walk reaches,
/// in the order it reaches them.
#[must_use]
pub fn check(document: &Document) -> (CheckedView, Diagnostics) {
  todo!("{document:?}")
}
