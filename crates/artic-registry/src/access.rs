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
/// A compiled view never calls them: it sets each property through its typed builder method.
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
  if tree.get(node).is_none() {
    return Err(ValueError::WrongNode);
  }
  tree.property(node, key).ok_or(ValueError::WrongNode)
}

/// What the loader would do with a property of a registry test: nothing, since those tests never
/// load a view.
#[cfg(test)]
pub(crate) static UNWRITTEN: PropertyAccess = PropertyAccess {
  set_text: |_, _, _, _| Err(ValueError::WrongNode),
  set_node: |_, _, _, _| Err(ValueError::WrongNode),
  set_null: |_, _, _| Err(ValueError::WrongNode),
  clear: |_, _, _| Err(ValueError::WrongNode),
};
