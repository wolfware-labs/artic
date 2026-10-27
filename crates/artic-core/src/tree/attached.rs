//! Attached property values, stored on the nodes they're set on.
//!
//! An attached property, such as `Grid.Row`, is declared by one type and set on nodes of others, so
//! the node has no field for it. Its value lives in the node's entry of [`AttachedValues`], keyed
//! by its [`AttachedKey`](crate::property::AttachedKey): created at the key's default the first time it's needed, read as that
//! default without storing anything, and connected to the node's dirty flags like a field.

use crate::arena::SlotMap;
use crate::property::{AnyProperty, Erased, KeyId};
use crate::tree::NodeId;

/// The attached property values of each node that has any.
#[derive(Debug, Default)]
pub(super) struct AttachedValues {
  values: SlotMap<Vec<Box<dyn Erased>>>,
}

impl AttachedValues {
  pub(super) const fn new() -> Self {
    Self { values: SlotMap::new() }
  }

  pub(super) fn find(&self, node: NodeId, key: KeyId) -> Option<AnyProperty<'_>> {
    self.of(node).find(|property| property.key_id() == key)
  }

  pub(super) fn of(&self, node: NodeId) -> impl Iterator<Item = AnyProperty<'_>> {
    self
      .values
      .get(node.0)
      .into_iter()
      .flatten()
      .map(|property| AnyProperty::new(property.as_ref()))
  }

  pub(super) fn insert(&mut self, node: NodeId, property: Box<dyn Erased>) {
    match self.values.get_mut(node.0) {
      Some(properties) => properties.push(property),
      None => self.values.insert(node.0, vec![property]),
    }
  }

  pub(super) fn remove(&mut self, node: NodeId) {
    self.values.remove(node.0);
  }

  /// How many attached properties `node` stores.
  #[cfg(test)]
  pub(super) fn count(&self, node: NodeId) -> usize {
    self.values.get(node.0).map_or(0, Vec::len)
  }
}
