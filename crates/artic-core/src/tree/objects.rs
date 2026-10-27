//! The objects themselves, and the ids that reach them.
//!
//! [`Objects`] keeps each node's `Box<dyn Object>` in a generational arena. It's the only part of
//! the tree that knows whether an id is live: the other parts only store data under ids it issued,
//! so `UiTree` checks an id here before it reaches them.

use crate::Object;
use crate::arena::Arena;
use crate::tree::NodeId;

/// Every node's object, in the slot its id names.
#[derive(Debug, Default)]
pub(super) struct Objects {
  arena: Arena<Box<dyn Object>>,
}

impl Objects {
  pub(super) const fn new() -> Self {
    Self { arena: Arena::new() }
  }

  pub(super) fn alloc(&mut self, object: Box<dyn Object>) -> NodeId {
    NodeId(self.arena.alloc(object))
  }

  pub(super) fn free(&mut self, id: NodeId) -> Option<Box<dyn Object>> {
    self.arena.free(id.0)
  }

  pub(super) fn get(&self, id: NodeId) -> Option<&dyn Object> {
    self.arena.get(id.0).map(AsRef::as_ref)
  }

  pub(super) fn get_mut(&mut self, id: NodeId) -> Option<&mut dyn Object> {
    self.arena.get_mut(id.0).map(AsMut::as_mut)
  }

  /// The object of `id`, which must be live.
  #[track_caller]
  pub(super) fn object(&self, id: NodeId) -> &dyn Object {
    match self.get(id) {
      Some(object) => object,
      None => panic!("node {id:?} is not in the arena"),
    }
  }

  /// The ids of every live node.
  pub(super) fn ids(&self) -> impl Iterator<Item = NodeId> + '_ {
    self.arena.ids().map(NodeId)
  }

  pub(super) fn len(&self) -> usize {
    self.arena.len()
  }

  pub(super) fn is_empty(&self) -> bool {
    self.arena.is_empty()
  }

  pub(super) fn contains(&self, id: NodeId) -> bool {
    self.arena.contains(id.0)
  }
}
