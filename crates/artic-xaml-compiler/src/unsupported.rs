//! The constructs a later issue adds, each reported with that issue.

use artic_xaml::diagnostics::Diagnostic;
use artic_xaml::infoset::TypeRef;

/// An `x:` primitive written as an element, such as `<x:String>`.
pub(crate) fn primitive(ty: &TypeRef) -> Diagnostic {
  Diagnostic::error(ty.span, "XAML primitives come with #62")
}

/// A value provider written as an element.
pub(crate) fn provider_as_element(ty: &TypeRef) -> Diagnostic {
  Diagnostic::error(ty.span, format!("`{}` is a value provider, not an element", ty.name))
    .with_help(format!("write it as an attribute value: `{{{} …}}`", ty.name))
}
