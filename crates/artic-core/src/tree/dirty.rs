//! Which frame stages each node needs redone.
//!
//! A node's dirty flags are an [`Affects`]: layout, render, or both. A new node starts with both. A
//! property change adds the stages its key affects, through the sink the tree gives each of the
//! node's properties; an edit to a node's visual children adds layout. The frame stages clear what they
//! redo: layout clears [`Affects::LAYOUT`], rendering clears [`Affects::RENDER`]. Every mark also sets
//! the tree's pending flag, which [`UiTree::take_pending`](super::UiTree::take_pending) reads and
//! clears. A mark that adds a queued stage to a node also queues the node for that stage, so the stage
//! finds its nodes through [`UiTree::take_dirty`](super::UiTree::take_dirty) without walking the
//! tree.
//!
//! A node drawn by its logical parent, such as an inline run, holds no flags of its own: its handle
//! passes every mark to the parent's, so whatever takes the parent's marks sees the change. See
//! [`UiTree::set_drawn_by_parent`](super::UiTree::set_drawn_by_parent).

use crate::arena::SlotMap;
use crate::property::Affects;
use crate::tree::NodeId;
use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

/// The stages whose newly marked nodes are queued.
const QUEUED: [Affects; 1] = [Affects::LAYOUT];

/// What every node's flags in one tree share: the pending flag and the queue of each queued stage.
#[derive(Debug, Default)]
struct Shared {
  pending: Cell<bool>,
  queues: [RefCell<Vec<NodeId>>; QUEUED.len()],
}

impl Shared {
  fn queue(&self, stage: Affects) -> &RefCell<Vec<NodeId>> {
    let index = QUEUED.iter().position(|queued| *queued == stage);
    &self.queues[index.expect("only a queued stage has a queue")]
  }
}

/// A node's dirty flags as its properties' sinks hold them, together with its tree's pending flag
/// and queues.
///
/// Marking the node dirty through it also sets the pending flag, so the frame pipeline learns that
/// a frame is needed without walking the tree. It's private to artic-core: the tree gives each
/// property a sink holding one when the node joins it, and disposes the properties when it frees
/// the node.
#[derive(Debug, Clone)]
pub struct DirtyHandle {
  node: NodeId,
  marks: Rc<NodeMarks>,
  shared: Rc<Shared>,
}

/// What every handle to one node shares.
#[derive(Debug, Default)]
struct NodeMarks {
  /// The stages the node needs redone.
  stages: Cell<Affects>,
  /// The parent's handle while the node is drawn by its parent, which then takes its marks.
  into: RefCell<Option<DirtyHandle>>,
}

impl DirtyHandle {
  /// Adds `stages` to the node's dirty flags and sets the tree's pending flag, as a property's sink
  /// does when its effective value changes, and queues the node for each queued stage it didn't
  /// have. Marking no stage does nothing.
  pub(crate) fn mark(&self, stages: Affects) {
    if stages.is_empty() {
      return;
    }
    if let Some(parent) = &*self.marks.into.borrow() {
      parent.mark(stages);
      return;
    }
    let added = stages.without(self.marks.stages.get());
    self.marks.stages.set(self.marks.stages.get() | stages);
    self.shared.pending.set(true);
    for stage in QUEUED.into_iter().filter(|stage| added.contains(*stage)) {
      self.shared.queue(stage).borrow_mut().push(self.node);
    }
  }

  pub(crate) fn stages(&self) -> Affects {
    self.marks.stages.get()
  }

  pub(crate) fn clear(&self, stages: Affects) {
    self.marks.stages.set(self.marks.stages.get().without(stages));
  }
}

/// Each node's dirty flags, shared with its properties' sinks, and the tree's pending flag and
/// queues.
#[derive(Debug, Default)]
pub(super) struct DirtyFlags {
  flags: SlotMap<DirtyHandle>,
  drawn_by_parent: SlotMap<()>,
  shared: OnceCell<Rc<Shared>>,
}

impl DirtyFlags {
  pub(super) const fn new() -> Self {
    Self {
      flags: SlotMap::new(),
      drawn_by_parent: SlotMap::new(),
      shared: OnceCell::new(),
    }
  }

  /// Gives `node` flags with layout and rendering dirty, and returns them.
  pub(super) fn insert(&mut self, node: NodeId) -> DirtyHandle {
    let shared = self.shared.get_or_init(Rc::default);
    let handle = DirtyHandle {
      node,
      marks: Rc::default(),
      shared: Rc::clone(shared),
    };
    handle.mark(Affects::LAYOUT | Affects::RENDER);
    self.flags.insert(node.0, handle.clone());
    handle
  }

  pub(super) fn remove(&mut self, node: NodeId) {
    self.flags.remove(node.0);
    self.drawn_by_parent.remove(node.0);
  }

  pub(super) fn is_drawn_by_parent(&self, node: NodeId) -> bool {
    self.drawn_by_parent.get(node.0).is_some()
  }

  pub(super) fn set_drawn_by_parent(&mut self, node: NodeId, drawn: bool) {
    if drawn {
      self.drawn_by_parent.insert(node.0, ());
    } else {
      self.drawn_by_parent.remove(node.0);
    }
  }

  /// Passes `node`'s marks to `parent` from now on, or keeps them on `node` for `None`. The marks
  /// `node` had move with them, so nothing it was marked for is lost.
  pub(super) fn redirect(&self, node: NodeId, parent: Option<NodeId>) {
    let Some(handle) = self.flags.get(node.0) else {
      return;
    };
    let into = parent.and_then(|parent| self.flags.get(parent.0)).cloned();
    if into.is_some() {
      let stages = handle.stages();
      handle.clear(stages);
      *handle.marks.into.borrow_mut() = into;
      handle.mark(stages);
    } else {
      *handle.marks.into.borrow_mut() = None;
    }
  }

  pub(super) fn get(&self, node: NodeId) -> Affects {
    self.flags.get(node.0).map_or(Affects::NONE, DirtyHandle::stages)
  }

  pub(super) fn mark(&self, node: NodeId, stages: Affects) {
    if let Some(handle) = self.flags.get(node.0) {
      handle.mark(stages);
    }
  }

  pub(super) fn clear(&self, node: NodeId, stages: Affects) {
    if let Some(handle) = self.flags.get(node.0) {
      handle.clear(stages);
    }
  }

  pub(super) fn shared(&self, node: NodeId) -> DirtyHandle {
    self.flags.get(node.0).expect("a live node has dirty flags").clone()
  }

  pub(super) fn take_pending(&self) -> bool {
    self.shared.get().is_some_and(|shared| shared.pending.replace(false))
  }

  /// Puts `nodes` back at the end of `stage`'s queue.
  pub(super) fn requeue(&self, stage: Affects, nodes: Vec<NodeId>) {
    if let Some(shared) = self.shared.get() {
      shared.queue(stage).borrow_mut().extend(nodes);
    }
  }

  /// Whether `stage`'s queue holds no node.
  pub(super) fn is_queue_empty(&self, stage: Affects) -> bool {
    self.shared.get().is_none_or(|shared| shared.queue(stage).borrow().is_empty())
  }

  /// Keeps in `stage`'s queue only the nodes `keep` accepts, in the order they were queued. A node
  /// may be in it twice, or no longer marked, or freed.
  pub(super) fn retain_queued(&self, stage: Affects, mut keep: impl FnMut(NodeId) -> bool) {
    if let Some(shared) = self.shared.get() {
      shared.queue(stage).borrow_mut().retain(|node| keep(*node));
    }
  }
}
