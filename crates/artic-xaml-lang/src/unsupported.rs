//! The XAML constructs a later issue adds, each reported with that issue.

use artic_diagnostics::{Diagnostic, Span};
use artic_xaml::infoset::{MarkupType, TypeRef};

/// An `x:` type written as an element: a primitive, such as `<x:String>`, or a value provider.
pub(crate) fn markup_element(ty: &TypeRef) -> Diagnostic {
  if ty.markup_type().is_some_and(MarkupType::is_markup_extension) {
    provider_as_element(ty)
  } else {
    primitive(ty.span)
  }
}

/// An `x:` primitive, written at `span`.
fn primitive(span: Span) -> Diagnostic {
  Diagnostic::error(span, "XAML primitives come with #62")
}

/// A value provider written as an element.
pub(crate) fn provider_as_element(ty: &TypeRef) -> Diagnostic {
  Diagnostic::error(ty.span, format!("`{ty}` is a value provider, not an element"))
    .with_help(format!("write it as an attribute value: `{{{ty} …}}`"))
}

/// An attached property, such as `Grid.Row="1"`.
pub(crate) fn attached(span: Span) -> Diagnostic {
  Diagnostic::error(span, "attached properties come with #50")
}

/// `x:Key`, which only a resource dictionary gives meaning to.
pub(crate) fn resource_key(span: Span) -> Diagnostic {
  Diagnostic::error(span, "resources come with #111")
}

/// A markup extension as a value: the issue that adds it, or an unknown value provider. An `x:`
/// primitive written as one comes with #62, as it does written as an element.
pub(crate) fn markup_extension(ty: &TypeRef, span: Span) -> Diagnostic {
  let comes_with = |issue: &str| Diagnostic::error(span, format!("`{{{ty}}}` comes with {issue}"));
  match ty.markup_type() {
    Some(markup) if markup.is_markup_extension() => comes_with("#62"),
    Some(_) => primitive(span),
    None => match ty.name.as_str() {
      "StaticResource" => comes_with("#112"),
      "Binding" => comes_with("#94"),
      "DynamicResource" => comes_with("#113"),
      "TemplateBinding" => comes_with("#85"),
      _ => Diagnostic::error(span, format!("unknown value provider `{ty}`")),
    },
  }
}
