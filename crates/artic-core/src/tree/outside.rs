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
  /// Each root asked for since `generation`, with the queued nodes it found outside it, except the one
  /// taking its queue, which [`Take`] holds until [`Outside::end`].
  roots: Vec<(Root<Q>, NodeMap<()>)>,
  /// For each queue, the nodes every root of it found outside it, off the queue.
  parked: Vec<(Q, NodeMap<()>)>,
  /// Emptied maps of roots from earlier generations, which keep their size for the next roots.
  spare: Vec<NodeMap<()>>,
}

/// One root's pass over its queue, from [`Outside::begin`] to [`Outside::end`].
#[derive(Debug)]
pub(super) struct Take<Q> {
  /// The root taking its queue.
  root: Root<Q>,
  /// The queued nodes the root found outside it, in this pass or an earlier one.
  found: NodeMap<()>,
  /// The nodes this pass took off the queue because every root found them outside it.
  parked: Vec<NodeId>,
}

/// Where a queued node stands for the root taking its queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Place {
  /// The node is under the root, which takes it.
  Within,
  /// The node is outside the root and may be under another, so it stays queued.
  Queued,
  /// Every root of the queue found the node outside it, so it leaves the queue.
  Parked,
}

impl<Q> Default for Outside<Q> {
  fn default() -> Self {
    Self::new()
  }
}

impl<Q> Outside<Q> {
  /// No root seen and nothing set aside.
  pub(super) const fn new() -> Self {
    Self {
      generation: 0,
      roots: Vec::new(),
      parked: Vec::new(),
      spare: Vec::new(),
    }
  }
}

impl<Q: Copy + Eq> Outside<Q> {
  /// Starts `root`'s pass at `generation`: returns the nodes that go back on its queue, every one
  /// parked there when the structure changed or `root` is new since it did, and the pass.
  pub(super) fn begin(&mut self, generation: u64, root: Root<Q>) -> (Vec<NodeId>, Take<Q>) {
    if self.generation != generation {
      self.generation = generation;
      self.spare.extend(self.roots.drain(..).map(|(_, mut found)| {
        found.retain(|_, ()| false);
        found
      }));
    }
    let (requeue, found) = match self.roots.iter().position(|(seen, _)| *seen == root) {
      Some(index) => (Vec::new(), self.roots.swap_remove(index).1),
      None => {
        let requeue = self.parked(root.0).drain().map(|(node, ())| node).collect();
        (requeue, self.spare.pop().unwrap_or_default())
      }
    };
    let take = Take {
      root,
      found,
      parked: Vec::new(),
    };
    (requeue, take)
  }

  /// Where `node` stands for `take`'s root; `within` says whether it's under the root, and is asked
  /// only when the root hasn't found it outside before.
  pub(super) fn place(&self, take: &mut Take<Q>, node: NodeId, within: impl FnOnce() -> bool) -> Place {
    if take.found.get(node).is_none() {
      if within() {
        return Place::Within;
      }
      take.found.insert(node, ());
    }
    if self.outside_the_others(take.root.0, node) {
      take.parked.push(node);
      return Place::Parked;
    }
    Place::Queued
  }

  /// Ends `take`: keeps what its root found outside it, and sets aside what it parked.
  pub(super) fn end(&mut self, take: Take<Q>) {
    let Take { root, found, parked } = take;
    self.roots.push((root, found));
    let queue = self.parked(root.0);
    for node in parked {
      queue.insert(node, ());
    }
  }

  /// The nodes parked off `queue`.
  fn parked(&mut self, queue: Q) -> &mut NodeMap<()> {
    let index = match self.parked.iter().position(|(parked, _)| *parked == queue) {
      Some(index) => index,
      None => {
        self.parked.push((queue, NodeMap::new()));
        self.parked.len() - 1
      }
    };
    &mut self.parked[index].1
  }

  /// Whether every root of `queue` other than the one taking it found `node` outside it.
  fn outside_the_others(&self, queue: Q, node: NodeId) -> bool {
    self
      .roots
      .iter()
      .filter(|(seen, _)| seen.0 == queue)
      .all(|(_, found)| found.get(node).is_some())
  }
}
