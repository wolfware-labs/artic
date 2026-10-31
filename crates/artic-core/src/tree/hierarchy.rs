//! The structure of the two trees: who is whose parent and child.
//!
//! Every node is in the logical tree and the visual tree at once, with a parent and ordered children
//! in each. A [`Tree`] holds one tree's links, and the UI tree keeps one per kind: it links and
//! unlinks nodes, checks for cycles, and walks children, ancestors and descendants. It knows
//! nothing about the other parts; the UI tree adds what an edit means for them.

use crate::arena::SlotMap;
use crate::tree::NodeId;
use std::iter::{from_fn, successors};

/// One tree's links: each live node's parent and ordered children.
#[derive(Debug, Default)]
pub(super) struct Tree {
  entries: SlotMap<Entry>,
}

#[derive(Debug, Default)]
struct Entry {
  parent: Option<NodeId>,
  children: Vec<NodeId>,
}

impl Tree {
  pub(super) const fn new() -> Self {
    Self { entries: SlotMap::new() }
  }

  /// Gives `node` an entry with no parent and no children.
  pub(super) fn insert(&mut self, node: NodeId) {
    self.entries.insert(node.0, Entry::default());
  }

  /// Drops `node`'s entry. The nodes it's linked to keep their links to it.
  pub(super) fn remove(&mut self, node: NodeId) {
    self.entries.remove(node.0);
  }

  pub(super) fn parent(&self, node: NodeId) -> Option<NodeId> {
    self.entry(node)?.parent
  }

  pub(super) fn children(&self, node: NodeId) -> &[NodeId] {
    self.entry(node).map(|entry| entry.children.as_slice()).unwrap_or_default()
  }

  pub(super) fn ancestors(&self, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    successors(self.parent(node), move |ancestor| self.parent(*ancestor))
  }

  pub(super) fn descendants(&self, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    let mut stack: Vec<NodeId> = self.children(node).iter().rev().copied().collect();
    from_fn(move || {
      let next = stack.pop()?;
      stack.extend(self.children(next).iter().rev());
      Some(next)
    })
  }

  /// Whether `id` is `root` or one of its descendants.
  pub(super) fn is_within(&self, id: NodeId, root: NodeId) -> bool {
    id == root || self.ancestors(id).any(|ancestor| ancestor == root)
  }

  /// Makes `child`, which has no parent, the child of `parent` at `index`.
  #[track_caller]
  pub(super) fn link(&mut self, parent: NodeId, index: usize, child: NodeId) {
    self.entry_mut(parent).children.insert(index, child);
    self.entry_mut(child).parent = Some(parent);
  }

  /// Cuts `node` from its parent and returns the parent, or `None` if it has none.
  #[track_caller]
  pub(super) fn unlink(&mut self, node: NodeId) -> Option<NodeId> {
    let parent = self.entry_mut(node).parent.take()?;
    self.entry_mut(parent).children.retain(|child| *child != node);
    Some(parent)
  }

  fn entry(&self, node: NodeId) -> Option<&Entry> {
    self.entries.get(node.0)
  }

  #[track_caller]
  fn entry_mut(&mut self, node: NodeId) -> &mut Entry {
    match self.entries.get_mut(node.0) {
      Some(entry) => entry,
      None => panic!("node {node:?} is not in the arena"),
    }
  }
}
