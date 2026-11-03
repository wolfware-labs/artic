use crate::tree::NodeId;

/// Values kept per node, found by the node's id without hashing.
///
/// It holds one value per slot of the tree: a value is found only by the exact id it went in
/// under, and setting one for a node that reuses a freed node's slot replaces the freed node's.
/// The map doesn't follow the tree, so a value stays until it's removed, replaced or
/// [`retain`](NodeMap::retain)ed away. It takes four bytes for each slot up to the highest it
/// holds a value for, and keeps the values themselves together, so a map of a few nodes in a large
/// tree stays small, and walking it costs what it holds.
#[derive(Debug)]
pub struct NodeMap<T> {
  positions: Vec<u32>,
  entries: Vec<(NodeId, T)>,
}

/// The position of a slot that holds no value.
const EMPTY: u32 = u32::MAX;

impl<T> Default for NodeMap<T> {
  fn default() -> Self {
    Self::new()
  }
}

impl<T> NodeMap<T> {
  #[must_use]
  pub const fn new() -> Self {
    Self {
      positions: Vec::new(),
      entries: Vec::new(),
    }
  }

  #[must_use]
  pub fn get(&self, node: NodeId) -> Option<&T> {
    self.position(node).map(|position| &self.entries[position].1)
  }

  pub fn get_mut(&mut self, node: NodeId) -> Option<&mut T> {
    self.position(node).map(|position| &mut self.entries[position].1)
  }

  /// Sets `node`'s value, and returns the value its slot held before with the node it was set for:
  /// `node`'s own, or a freed node's.
  pub fn insert(&mut self, node: NodeId, value: T) -> Option<(NodeId, T)> {
    let slot = node.0.index();
    if slot >= self.positions.len() {
      self.positions.resize(slot + 1, EMPTY);
    }
    match self.positions[slot] {
      EMPTY => {
        self.positions[slot] = u32::try_from(self.entries.len()).expect("fewer values than u32::MAX");
        self.entries.push((node, value));
        None
      }
      position => Some(std::mem::replace(&mut self.entries[position as usize], (node, value))),
    }
  }

  pub fn remove(&mut self, node: NodeId) -> Option<T> {
    let position = self.position(node)?;
    Some(self.remove_at(position))
  }

  /// The value under `node`, setting the default first if it has none, in place of any value a
  /// freed node left in the slot.
  pub fn get_or_default(&mut self, node: NodeId) -> &mut T
  where
    T: Default,
  {
    if self.position(node).is_none() {
      self.insert(node, T::default());
    }
    let position = self.positions[node.0.index()] as usize;
    &mut self.entries[position].1
  }

  /// Keeps only the values `keep` accepts, handing it each value's node.
  pub fn retain(&mut self, mut keep: impl FnMut(NodeId, &mut T) -> bool) {
    let mut position = 0;
    while position < self.entries.len() {
      let (node, value) = &mut self.entries[position];
      if keep(*node, value) {
        position += 1;
      } else {
        self.remove_at(position);
      }
    }
  }

  pub fn iter_mut(&mut self) -> impl Iterator<Item = (NodeId, &mut T)> {
    self.entries.iter_mut().map(|(node, value)| (*node, value))
  }

  fn position(&self, node: NodeId) -> Option<usize> {
    let position = *self.positions.get(node.0.index())? as usize;
    (position != EMPTY as usize && self.entries[position].0 == node).then_some(position)
  }

  fn remove_at(&mut self, position: usize) -> T {
    let (node, value) = self.entries.swap_remove(position);
    self.positions[node.0.index()] = EMPTY;
    if let Some((moved, _)) = self.entries.get(position) {
      self.positions[moved.0.index()] = position as u32;
    }
    value
  }
}
