//! Shows an error to a person: its message, then each cause.

use std::fmt::{self, Display, Formatter};

/// Shows `error`'s message, then each [`source`](std::error::Error::source), each after a `: `.
///
/// An error's message never formats its cause, so code that shows an error to a person, in a
/// panic, a log line or a `main` that returns it, shows `Chain(&error)` rather than the error.
#[derive(Clone, Copy, Debug)]
pub struct Chain<'a>(pub &'a dyn std::error::Error);

impl Display for Chain<'_> {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    Display::fmt(self.0, f)?;
    std::iter::successors(self.0.source(), |cause| cause.source()).try_for_each(|cause| write!(f, ": {cause}"))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[derive(Debug, thiserror::Error)]
  #[error("outer")]
  struct Outer(#[source] Middle);

  #[derive(Debug, thiserror::Error)]
  #[error("middle")]
  struct Middle(#[source] std::io::Error);

  // A chain shows the message, then each cause in order, each once.
  #[test]
  fn chain_shows_each_cause_once() {
    let error = Outer(Middle(std::io::Error::other("inner")));

    assert_eq!(Chain(&error).to_string(), "outer: middle: inner");
  }

  // An error without a cause shows its message alone.
  #[test]
  fn chain_of_an_error_without_a_cause_is_its_message() {
    assert_eq!(Chain(&std::io::Error::other("alone")).to_string(), "alone");
  }
}
