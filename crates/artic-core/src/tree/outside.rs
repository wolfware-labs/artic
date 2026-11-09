//! What [`UiTree::take_dirty`](super::UiTree::take_dirty) and
//! [`UiTree::take_relinked`](super::UiTree::take_relinked) know about the queued nodes under no
//! root, as of one structure generation: the nodes each root found outside it, and the nodes every
//! root found outside, which leave their queue so that frames stop visiting them.

use super::{NodeId, NodeMap, TreeKind};

/// A root a queue is taken for: the queue, the tree the root is looked in, and the root node.
type Root<Q> = (Q, TreeKind, NodeId);

/// The roots seen and the nodes set aside since the structure last changed, for the queues `Q`
/// names: a dirty stage, or the tree whose links are queued.
#[derive(Debug)]
pub(super) struct Outside<Q> {
  /// The structure generation the rest holds for.
  generation: u64,
  /// Each root asked for since `generation`, with the queued nodes it found outside it.
  roots: Vec<(Root<Q>, NodeMap<()>)>,
  /// The nodes every root of their queue found outside it, off that queue.
  parked: Vec<(Q, NodeId)>,
}

impl<Q> Default for Outside<Q> {
  fn default() -> Self {
    Self::new()
  }
}

impl<Q> Outside<Q> {
  /// No root seen and nothing set aside.
  pub(super) const fn new() -> Outside<Q> {
    Outside {
      generation: 0,
      roots: Vec::new(),
      parked: Vec::new(),
    }
  }
}

impl<Q: Copy + Eq> Outside<Q> {
  /// Starts a take for `root` at `generation`: returns the nodes that go back on `root`'s queue, every parked one when the structure changed or `root` is new since it did, and the
  /// nodes `root` found outside it before.
  pub(super) fn begin(&mut self, generation: u64, root: Root<Q>) -> (Vec<NodeId>, NodeMap<()>) {
    if self.generation != generation {
      self.generation = generation;
      self.roots.clear();
    }
    let mut requeue = Vec::new();
    let found = match self.roots.iter_mut().find(|(seen, _)| *seen == root) {
      Some((_, found)) => std::mem::take(found),
      None => {
        self.roots.push((root, NodeMap::default()));
        self.parked.retain(|(stage, node)| {
          if *stage == root.0 {
            requeue.push(*node);
            return false;
          }
          true
        });
        NodeMap::default()
      }
    };
    (requeue, found)
  }

  /// Whether every root of `root`'s queue other than `root` found `node` outside it.
  pub(super) fn outside_the_others(&self, root: Root<Q>, node: NodeId) -> bool {
    self
      .roots
      .iter()
      .filter(|(seen, _)| *seen != root && seen.0 == root.0)
      .all(|(_, found)| found.get(node).is_some())
  }

  /// Ends the take for `root`: keeps what it found outside it, and sets `parked` aside.
  pub(super) fn end(&mut self, root: Root<Q>, found: NodeMap<()>, parked: Vec<NodeId>) {
    if let Some((_, slot)) = self.roots.iter_mut().find(|(seen, _)| *seen == root) {
      *slot = found;
    }
    self.parked.extend(parked.into_iter().map(|node| (root.0, node)));
  }
}
