use crate::arena::SlotMap;
use crate::tree::NodeId;

/// Values kept per node, found by the node's id without hashing.
///
/// A value is found only by the exact id it went in under: once the tree frees a node and reuses
/// its slot, the new node finds nothing until a value is set for it. The map doesn't follow the
/// tree, so a value stays until it's removed, replaced or [`retain`](NodeMap::retain)ed away.
#[derive(Debug)]
pub struct NodeMap<T>(SlotMap<T>);

impl<T> Default for NodeMap<T> {
  fn default() -> Self {
    Self::new()
  }
}

impl<T> NodeMap<T> {
  #[must_use]
  pub const fn new() -> Self {
    Self(SlotMap::new())
  }

  #[must_use]
  pub fn get(&self, node: NodeId) -> Option<&T> {
    self.0.get(node.0)
  }

  pub fn get_mut(&mut self, node: NodeId) -> Option<&mut T> {
    self.0.get_mut(node.0)
  }

  pub fn insert(&mut self, node: NodeId, value: T) {
    self.0.insert(node.0, value);
  }

  pub fn remove(&mut self, node: NodeId) -> Option<T> {
    self.0.remove(node.0)
  }

  /// The value under `node`, inserting the default first if it has none.
  pub fn get_or_default(&mut self, node: NodeId) -> &mut T
  where
    T: Default,
  {
    self.0.get_or_insert_with(node.0, T::default)
  }

  /// Keeps only the values `keep` accepts, handing it each value's node.
  pub fn retain(&mut self, mut keep: impl FnMut(NodeId, &mut T) -> bool) {
    self.0.retain(|id, value| keep(NodeId(id), value));
  }

  pub fn iter_mut(&mut self) -> impl Iterator<Item = (NodeId, &mut T)> {
    self.0.iter_mut().map(|(id, value)| (NodeId(id), value))
  }

  pub fn clear(&mut self) {
    self.0.clear();
  }
}
