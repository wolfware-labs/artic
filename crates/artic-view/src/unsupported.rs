//! The constructs a later issue adds, which the checker reports with that issue in every
//! language. A front end reports its own syntax's constructs itself.

use artic_diagnostics::{Diagnostic, Span};

/// An event handler set where a property is, at the member's name.
#[must_use]
pub fn event(span: Span) -> Diagnostic {
  Diagnostic::error(span, "event handlers come with #82")
}

/// An element written as a property's value, such as a brush written as an element.
#[must_use]
pub fn value_object(span: Span) -> Diagnostic {
  Diagnostic::error(span, "value objects come with #57")
}

/// Content for a collection of values, such as gradient stops.
#[must_use]
pub fn value_collection(span: Span) -> Diagnostic {
  Diagnostic::error(span, "value collections come with #57")
}
