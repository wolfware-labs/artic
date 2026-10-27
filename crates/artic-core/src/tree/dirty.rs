//! Which frame stages each node needs redone.
//!
//! A node's dirty flags are an [`Affects`]: layout, render, or both. A new node starts with both. A
//! property change adds the stages its key affects, through flags the node shares with its
//! properties; an edit to a node's visual children adds layout. The frame stages clear what they
//! redo: layout clears [`Affects::LAYOUT`], rendering clears [`Affects::RENDER`]. Every mark also sets
//! the tree's pending flag, which [`UiTree::take_pending`](super::UiTree::take_pending) reads and
//! clears.

use crate::arena::SlotMap;
use crate::property::Affects;
use crate::tree::NodeId;
use std::cell::{Cell, OnceCell};
use std::rc::Rc;

/// A node's dirty flags as its properties hold them, together with its tree's pending flag.
///
/// Marking the node dirty through it also sets the pending flag, so the frame pipeline learns that
/// a frame is needed without walking the tree. Once the node is freed, marking through it does
/// nothing.
#[derive(Debug, Clone, Default)]
pub struct DirtyHandle {
  stages: Rc<Cell<Affects>>,
  pending: Rc<Cell<bool>>,
  freed: Rc<Cell<bool>>,
}

impl DirtyHandle {
  /// Adds `stages` to the node's dirty flags and sets the tree's pending flag, as a property does
  /// when its effective value changes. Marking no stage, or marking a freed node, does nothing.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::Object;
  /// use artic_core::property::{Affects, AnyProperty, KeyId};
  /// use artic_core::tree::{DirtyHandle, UiTree};
  /// use std::any::Any;
  /// use std::cell::RefCell;
  ///
  /// /// A property kept outside `Property`, which marks its node itself.
  /// #[derive(Debug, Default)]
  /// struct Custom(RefCell<Option<DirtyHandle>>);
  ///
  /// impl AnyProperty for Custom {
  ///   fn key_id(&self) -> KeyId {
  ///     unimplemented!()
  ///   }
  ///   fn inherits(&self) -> bool {
  ///     false
  ///   }
  ///   fn attach(&self, dirty: DirtyHandle) {
  ///     *self.0.borrow_mut() = Some(dirty);
  ///   }
  ///   fn inherit_from(&self, _source: Option<&dyn AnyProperty>) {}
  ///   fn dispose(&self) {}
  ///   fn as_any(&self) -> &dyn Any {
  ///     self
  ///   }
  /// }
  ///
  /// #[derive(Debug, Default)]
  /// struct Node(Custom);
  ///
  /// impl Object for Node {
  ///   fn visit_properties(&self, visit: &mut dyn FnMut(&dyn AnyProperty)) {
  ///     visit(&self.0);
  ///   }
  /// }
  ///
  /// let mut tree = UiTree::new();
  /// let id = tree.alloc(Box::new(Node::default()));
  /// tree.clear_dirty(id, Affects::LAYOUT | Affects::RENDER);
  /// let _ = tree.take_pending();
  ///
  /// let node = tree.get_as::<Node>(id).unwrap();
  /// node.0.0.borrow().as_ref().unwrap().mark(Affects::RENDER);
  ///
  /// assert_eq!(tree.dirty(id), Affects::RENDER);
  /// assert!(tree.take_pending());
  /// ```
  pub fn mark(&self, stages: Affects) {
    if stages.is_empty() || self.freed.get() {
      return;
    }
    self.stages.set(self.stages.get() | stages);
    self.pending.set(true);
  }

  pub(crate) fn stages(&self) -> Affects {
    self.stages.get()
  }

  pub(crate) fn clear(&self, stages: Affects) {
    self.stages.set(self.stages.get().without(stages));
  }
}

/// Each node's dirty flags, shared with its properties, and the tree's pending flag.
#[derive(Debug, Default)]
pub(super) struct DirtyFlags {
  flags: SlotMap<DirtyHandle>,
  pending: OnceCell<Rc<Cell<bool>>>,
}

impl DirtyFlags {
  pub(super) const fn new() -> Self {
    Self {
      flags: SlotMap::new(),
      pending: OnceCell::new(),
    }
  }

  /// Gives `node` new flags with layout and rendering dirty, which sets the pending flag, and
  /// returns them for its properties to share.
  pub(super) fn insert(&mut self, node: NodeId) -> DirtyHandle {
    let pending = self.pending.get_or_init(|| Rc::new(Cell::new(false)));
    let handle = DirtyHandle {
      stages: Rc::new(Cell::new(Affects::NONE)),
      pending: Rc::clone(pending),
      freed: Rc::new(Cell::new(false)),
    };
    handle.mark(Affects::LAYOUT | Affects::RENDER);
    self.flags.insert(node.0, handle.clone());
    handle
  }

  /// Drops `node`'s flags. Its properties keep their own handle to them, which marks nothing from
  /// then on.
  pub(super) fn remove(&mut self, node: NodeId) {
    if let Some(handle) = self.flags.remove(node.0) {
      handle.freed.set(true);
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
    self.pending.get().is_some_and(|pending| pending.replace(false))
  }
}
