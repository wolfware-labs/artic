//! Input delivered to elements.
//!
//! [`InputEvent`] is a placeholder: it carries nothing yet. Pointer, keyboard and text input arrive
//! with input routing (M3), which adds them without changing how elements receive events.

/// One input event delivered to an element, such as a pointer press or a key.
///
/// A placeholder until input routing (M3). It's named apart from [`Event`](crate::event::Event),
/// the field type that declares a node's events.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct InputEvent {}

#[cfg(test)]
mod tests {
  use super::*;
  use static_assertions::assert_impl_all;

  assert_impl_all!(InputEvent: Clone, Default, Send, Sync);
}
