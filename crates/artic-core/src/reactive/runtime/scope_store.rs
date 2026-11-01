use crate::arena::{Arena, SlotId};
use crate::reactive::runtime::Node;
use std::cell::RefCell;
use std::mem::take;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct ScopeId(SlotId);

struct ScopeSlot {
  parent: Option<ScopeId>,
  children: Vec<ScopeId>,
  /// The newest node the scope owns, the head of a list through the nodes' slots.
  first: Option<Node>,
}

pub struct ScopeStore {
  slots: RefCell<Arena<ScopeSlot>>,
  active: RefCell<Vec<ScopeId>>,
}

impl ScopeStore {
  pub const fn new() -> Self {
    Self {
      slots: RefCell::new(Arena::new()),
      active: RefCell::new(Vec::new()),
    }
  }

  pub fn alloc(&self, parent: Option<ScopeId>) -> ScopeId {
    let scope_id = ScopeId(self.slots.borrow_mut().alloc(ScopeSlot {
      parent,
      children: Vec::new(),
      first: None,
    }));
    if let Some(parent) = parent {
      self.with_slot(parent, |slot| slot.children.push(scope_id));
    }
    scope_id
  }

  pub fn free(&self, scope_id: ScopeId) {
    self.slots.borrow_mut().free(scope_id.0);
  }

  pub fn contains(&self, scope_id: ScopeId) -> bool {
    self.slots.borrow().contains(scope_id.0)
  }

  pub fn count(&self) -> usize {
    self.slots.borrow().len()
  }

  pub fn parent(&self, scope_id: ScopeId) -> Option<ScopeId> {
    self.with_slot(scope_id, |slot| slot.parent)
  }

  pub fn remove_child(&self, scope_id: ScopeId, child_id: ScopeId) {
    if let Some(slot) = self.slots.borrow_mut().get_mut(scope_id.0) {
      slot.children.retain(|child| *child != child_id);
    }
  }

  #[cfg(test)]
  pub fn child_count(&self, scope_id: ScopeId) -> usize {
    self.with_slot(scope_id, |slot| slot.children.len())
  }

  pub fn take_children(&self, scope_id: ScopeId) -> Vec<ScopeId> {
    self.with_slot(scope_id, |slot| take(&mut slot.children))
  }

  pub fn first(&self, scope_id: ScopeId) -> Option<Node> {
    self.with_slot(scope_id, |slot| slot.first)
  }

  /// Makes `first` the head of `scope_id`'s list. A scope already gone has no list to change: a node
  /// made while its scope was being freed outlives it.
  pub fn set_first(&self, scope_id: ScopeId, first: Option<Node>) {
    if let Some(slot) = self.slots.borrow_mut().get_mut(scope_id.0) {
      slot.first = first;
    }
  }

  pub fn push_active(&self, scope_id: ScopeId) {
    self.active.borrow_mut().push(scope_id);
  }

  pub fn pop_active(&self) {
    self.active.borrow_mut().pop();
  }

  pub fn active(&self) -> Option<ScopeId> {
    self.active.borrow().last().copied()
  }

  fn with_slot<TResult>(&self, scope_id: ScopeId, action: impl FnOnce(&mut ScopeSlot) -> TResult) -> TResult {
    let mut slots = self.slots.borrow_mut();
    action(slots.get_mut(scope_id.0).expect("scope slot not found for the specified id"))
  }
}
