//! The constructs a later issue adds, each reported with that issue.

use artic_diagnostics::{Diagnostic, Span};
use artic_xaml::infoset::{TypeRef, XamlNamespace};

/// An `x:` primitive written as an element, such as `<x:String>`.
pub(crate) fn primitive(ty: &TypeRef) -> Diagnostic {
  Diagnostic::error(ty.span, "XAML primitives come with #62")
}

/// A value provider written as an element.
pub(crate) fn provider_as_element(ty: &TypeRef) -> Diagnostic {
  Diagnostic::error(ty.span, format!("`{}` is a value provider, not an element", ty.name))
    .with_help(format!("write it as an attribute value: `{{{} …}}`", ty.name))
}

/// An event handler, such as `Click="on_click"`.
pub(crate) fn event(span: Span) -> Diagnostic {
  Diagnostic::error(span, "event handlers come with #82")
}

/// An attached property, such as `Grid.Row="1"`.
pub(crate) fn attached(span: Span) -> Diagnostic {
  Diagnostic::error(span, "attached properties come with #50")
}

/// An object as a property's value, such as a brush written as an element.
pub(crate) fn value_object(span: Span) -> Diagnostic {
  Diagnostic::error(span, "value objects come with #57")
}

/// `x:Key`, which only a resource dictionary gives meaning to.
pub(crate) fn resource_key(span: Span) -> Diagnostic {
  Diagnostic::error(span, "resources come with #111")
}

/// A markup extension as a value: the issue that adds it, or an unknown value provider.
pub(crate) fn markup_extension(ty: &TypeRef, span: Span) -> Diagnostic {
  let written = match &ty.prefix {
    Some(prefix) => format!("{prefix}:{}", ty.name),
    None => ty.name.clone(),
  };
  let issue = match (&ty.namespace, ty.name.as_str()) {
    (XamlNamespace::Markup, "Static" | "Null" | "Type") => Some("#62"),
    (XamlNamespace::Markup, _) => None,
    (_, "StaticResource") => Some("#112"),
    (_, "Binding") => Some("#94"),
    (_, "DynamicResource") => Some("#113"),
    (_, "TemplateBinding") => Some("#85"),
    _ => None,
  };
  match issue {
    Some(issue) => Diagnostic::error(span, format!("`{{{written}}}` comes with {issue}")),
    None => Diagnostic::error(span, format!("unknown value provider `{written}`")),
  }
}

/// Content for a collection of values, such as gradient stops.
pub(crate) fn value_collection(span: Span) -> Diagnostic {
  Diagnostic::error(span, "value collections come with #57")
}
