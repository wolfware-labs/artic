//! The constructs a later issue adds, each reported with that issue.

use artic_xaml::diagnostics::Diagnostic;
use artic_xaml::infoset::TypeRef;

/// An `x:` primitive written as an element, such as `<x:String>`.
pub(crate) fn primitive(ty: &TypeRef) -> Diagnostic {
  Diagnostic::error(ty.span, "XAML primitives come with #62")
}
