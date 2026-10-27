//! How inheritable properties take their value from the logical tree.
//!
//! A property whose key inherits, such as `FontSize`, follows the same property on its nearest
//! logical ancestor that has one, as a field or an attached value: that ancestor's value fills its
//! `Inherited` level. This file finds that ancestor for every inheritable property of a subtree,
//! and is run again whenever a logical edit moves the subtree, so each property follows its new
//! ancestor. Nodes without the property are skipped, and inheritance never follows the visual tree.
//!
//! It holds no state of its own: it reads the objects, their attached values and the logical
//! tree's links. [`with_property`], the lookup of a node's property by key it walks with, also
//! serves [`UiTree::attached`](super::UiTree::attached).

use crate::object::for_each_property;
use crate::property::{AnyProperty, KeyId};
use crate::tree::NodeId;
use crate::tree::attached::AttachedValues;
use crate::tree::hierarchy::Tree;
use crate::tree::objects::Objects;
use std::iter::once;

/// Makes each inheritable property of `root` and its logical descendants, fields and attached
/// values alike, follow its nearest logical ancestor.
pub(super) fn relink(objects: &Objects, attached: &AttachedValues, logical: &Tree, root: NodeId) {
  for node in once(root).chain(logical.descendants(root)) {
    let mut follow = |property: AnyProperty<'_>| {
      if property.inherits() {
        inherit_from_nearest_ancestor(objects, attached, logical, node, property);
      }
    };
    for_each_property(objects.object(node), &mut follow);
    attached.of(node).for_each(follow);
  }
}

fn inherit_from_nearest_ancestor(
  objects: &Objects,
  attached: &AttachedValues,
  logical: &Tree,
  node: NodeId,
  property: AnyProperty<'_>,
) {
  let key = property.key_id();
  let followed = logical.ancestors(node).find_map(|ancestor| {
    with_property(objects, attached, ancestor, key, |candidate| {
      property.inherit_from(Some(candidate))
    })
  });
  if followed.is_none() {
    property.inherit_from(None);
  }
}

/// Calls `function` with `node`'s property of the kind `key`: its field of that kind, else its
/// attached value. Returns `None`, without calling it, when the node has neither.
///
/// # Panics
///
/// Panics if `node` is stale.
#[track_caller]
pub(super) fn with_property<R>(
  objects: &Objects,
  attached: &AttachedValues,
  node: NodeId,
  key: KeyId,
  function: impl FnOnce(AnyProperty<'_>) -> R,
) -> Option<R> {
  let mut function = Some(function);
  let mut result = None;
  for_each_property(objects.object(node), |property| {
    if property.key_id() == key
      && let Some(function) = function.take()
    {
      result = Some(function(property));
    }
  });
  match function {
    Some(function) => attached.find(node, key).map(function),
    None => result,
  }
}
