//! How a UI language's runtime loader writes a property without naming its value type
//! ([loader values design](https://github.com/wolfware-labs/artic/blob/main/docs/design/loader-values.md)).

use crate::ValueError;
use artic_core::property::{Precedence, Property, PropertyKey};
use artic_core::tree::{NodeId, UiTree};
use std::fmt::{self, Debug, Formatter};

/// The functions that write one property. Their signatures name no value type and their bodies are
/// typed: `#[derive(UiType)]` and `#[derive(UiProperties)]` write them for each property, and each
/// finds its property on the node by its key, so a property of a group is written the same way on
/// every type that holds the group.
///
/// A property is found as the node's field of that key, or else its attached value of that key, as
/// [`UiTree::property`] finds it, so an access whose key is an attached key, such as a group's
/// property that shares one, also writes a node that holds the attached value.
///
/// A compiled view never calls them: it sets each property through its typed builder method.
///
/// # Panics
///
/// An entry that reaches the property panics when given a level no caller sets directly:
/// `Precedence::Inherited`, which the tree fills, and for `clear` also `Precedence::Default`, which a
/// property always has, as [`Property::set_at`] and [`Property::clear`] do. One that refuses its
/// value first returns the error instead. A loader passes `Local` or `StyleSetter`.
pub struct PropertyAccess {
  /// Parses `text` with the value type's `FromStr` and sets the value at `level`. For a
  /// `Property<Option<V>>` it parses a `V`.
  pub set_text: fn(&mut UiTree, NodeId, &str, Precedence) -> Result<(), ValueError>,
  /// Sets a node as the value, for a property that holds one, such as `Property<NodeId>` or
  /// `Property<Option<NodeId>>`. A property of another value type refuses it with
  /// [`ValueError::WrongType`].
  pub set_node: fn(&mut UiTree, NodeId, NodeId, Precedence) -> Result<(), ValueError>,
  /// Sets `None` at `level`, for a `Property<Option<T>>`. A property of another value type refuses
  /// it with [`ValueError::NotNullable`].
  pub set_null: fn(&mut UiTree, NodeId, Precedence) -> Result<(), ValueError>,
  /// Empties `level`, when a style stops matching or a reload removes the value.
  pub clear: fn(&mut UiTree, NodeId, Precedence) -> Result<(), ValueError>,
}

impl Debug for PropertyAccess {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    formatter.debug_struct("PropertyAccess").finish_non_exhaustive()
  }
}

/// `node`'s property of the kind `key`, or [`ValueError::WrongNode`] when the node has none or is no
/// longer in the tree.
pub(crate) fn property_of<T>(tree: &UiTree, node: NodeId, key: &'static PropertyKey<T>) -> Result<Property<T>, ValueError>
where
  T: PartialEq + 'static,
{
  if !tree.contains(node) {
    return Err(ValueError::WrongNode);
  }
  tree.property(node, key).ok_or(ValueError::WrongNode)
}

#[cfg(test)]
pub(crate) mod tests {
  use super::*;

  /// What the loader would do with a property of a registry test: nothing, since those tests
  /// never load a view.
  pub(crate) static UNWRITTEN: PropertyAccess = PropertyAccess {
    set_text: |_, _, _, _| Err(ValueError::WrongNode),
    set_node: |_, _, _, _| Err(ValueError::WrongNode),
    set_null: |_, _, _| Err(ValueError::WrongNode),
    clear: |_, _, _| Err(ValueError::WrongNode),
  };
}
