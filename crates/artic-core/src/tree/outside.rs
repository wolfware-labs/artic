//! What [`UiTree::take_dirty`](super::UiTree::take_dirty) knows about the queued nodes under no
//! root, as of one structure generation: the nodes each root found outside it, and the nodes every
//! root found outside, which leave the queue so that frames stop visiting them.

use super::{NodeId, NodeMap, TreeKind};
use crate::property::Affects;

/// A root a queue is taken for: the stage, the tree it's looked in, and the root node.
type Root = (Affects, TreeKind, NodeId);

/// The roots seen and the nodes set aside since the structure last changed.
#[derive(Debug, Default)]
pub(super) struct Outside {
  /// The structure generation the rest holds for.
  generation: u64,
  /// Each root asked for since `generation`, with the queued nodes it found outside it.
  roots: Vec<(Root, NodeMap<()>)>,
  /// The nodes every root of their stage found outside it, off their stage's queue.
  parked: Vec<(Affects, NodeId)>,
}

impl Outside {
  /// No root seen and nothing set aside.
  pub(super) const fn new() -> Outside {
    Outside {
      generation: 0,
      roots: Vec::new(),
      parked: Vec::new(),
    }
  }

  /// Starts a take for `root` at `generation`: returns the nodes of `root`'s stage that go back on
  /// its queue, every parked one when the structure changed or `root` is new since it did, and the
  /// nodes `root` found outside it before.
  pub(super) fn begin(&mut self, generation: u64, root: Root) -> (Vec<NodeId>, NodeMap<()>) {
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

  /// Whether every root of `root`'s stage other than `root` found `node` outside it.
  pub(super) fn outside_the_others(&self, root: Root, node: NodeId) -> bool {
    self
      .roots
      .iter()
      .filter(|(seen, _)| *seen != root && seen.0 == root.0)
      .all(|(_, found)| found.get(node).is_some())
  }

  /// Ends the take for `root`: keeps what it found outside it, and sets `parked` aside.
  pub(super) fn end(&mut self, root: Root, found: NodeMap<()>, parked: Vec<NodeId>) {
    if let Some((_, slot)) = self.roots.iter_mut().find(|(seen, _)| *seen == root) {
      *slot = found;
    }
    self.parked.extend(parked.into_iter().map(|node| (root.0, node)));
  }
}
