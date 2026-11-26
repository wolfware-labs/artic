//! Why a UI language's value or child was refused: the errors of the runtime loader
//! ([loader design](https://github.com/wolfware-labs/artic/blob/main/docs/design/loader-values.md)),
//! which the content route already returns.

use artic_core::tree::{LinkError, NodeId};

/// Why a value or a child element from a UI language was refused. Positions are added by the loader.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ValueError {
  /// The text didn't parse. `message` is the value type's `FromStr` error, displayed.
  #[error("expected {expected}: {message}")]
  Parse {
    /// The type the text should have parsed as.
    expected: &'static str,
    /// The parser's message.
    message: String,
  },
  /// A null value for a property whose type isn't `Option<T>`.
  #[error("{property} can't be null")]
  NotNullable {
    /// The property's name.
    property: &'static str,
  },
  /// An object or resource of the wrong type.
  #[error("expected {expected}, found {found}")]
  WrongType {
    /// The type the property takes.
    expected: &'static str,
    /// The type it was given.
    found: &'static str,
  },
  /// The target node isn't of the property's or the content's owner type.
  #[error("the node isn't of the property's or content's owner type")]
  WrongNode,
  /// The child can't be linked to its parent.
  #[error("can't add the child")]
  InvalidChild(#[source] LinkError),
  /// An object given as a staged value that isn't one: a stale id, the owner itself, or a node
  /// linked in either tree. Only a detached leaf is taken as a value.
  #[error("the object isn't a staged value")]
  NotStaged(NodeId),
  /// A second child for an element that takes one.
  #[error("this element takes a single child")]
  OneChildOnly,
  /// A child of a kind the content doesn't take.
  #[error("this element's content takes only {expected}")]
  UnexpectedChild {
    /// The types the content takes, in prose, such as "`Run`, `Bold` or `Italic`".
    expected: String,
  },
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::collection::tests::Leaf;

  // Null is refused in the registry's words, which no UI language owns.
  #[test]
  fn not_nullable_names_no_language() {
    let error = ValueError::NotNullable { property: "Background" };

    assert_eq!(error.to_string(), "Background can't be null");
  }

  // A child of a kind the content doesn't take names what it takes.
  #[test]
  fn unexpected_child_names_what_the_content_takes() {
    let error = ValueError::UnexpectedChild {
      expected: "`Item`".to_owned(),
    };

    assert_eq!(error.to_string(), "this element's content takes only `Item`");
  }

  // An object that isn't a staged value is refused in the registry's words, which no UI language
  // owns.
  #[test]
  fn not_staged_names_no_language() {
    let id = artic_core::tree::UiTree::new().alloc(Box::new(Leaf));

    assert_eq!(ValueError::NotStaged(id).to_string(), "the object isn't a staged value");
  }

  // A refused child says so, and the link error is its source.
  #[test]
  fn invalid_child_shows_its_cause_once() {
    use std::error::Error as _;
    let id = artic_core::tree::UiTree::new().alloc(Box::new(Leaf));
    let error = ValueError::InvalidChild(LinkError::NotInTree(id));

    assert_eq!(error.to_string(), "can't add the child");
    assert_eq!(
      error.source().map(ToString::to_string),
      Some(LinkError::NotInTree(id).to_string())
    );
  }
}
