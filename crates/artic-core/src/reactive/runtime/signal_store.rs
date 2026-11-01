use crate::arena::{Arena, SlotId};
use crate::reactive::runtime::value_cell::ValueCell;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct SignalId(SlotId);

struct SignalSlot {
  value: ValueCell,
}

impl SignalSlot {
  fn new(value: Box<dyn Any>) -> Self {
    Self {
      value: ValueCell::new(value, "signal"),
    }
  }
}

pub struct SignalStore {
  slots: RefCell<Arena<Rc<SignalSlot>>>,
}

impl SignalStore {
  pub const fn new() -> Self {
    Self {
      slots: RefCell::new(Arena::new()),
    }
  }

  pub fn alloc_value<TValue>(&self, value: TValue) -> SignalId
  where
    TValue: Any,
  {
    SignalId(self.slots.borrow_mut().alloc(Rc::new(SignalSlot::new(Box::new(value)))))
  }

  pub fn with_signal_value<TValue, TResult>(&self, signal_id: SignalId, action: impl FnOnce(&TValue) -> TResult) -> TResult
  where
    TValue: Any,
  {
    self.get_slot(signal_id).value.with(action)
  }

  pub fn with_signal_mut_value<TValue, TResult>(
    &self,
    signal_id: SignalId,
    action: impl FnOnce(&mut TValue) -> TResult,
  ) -> TResult
  where
    TValue: Any,
  {
    self.get_slot(signal_id).value.with_mut(action)
  }

  pub fn free(&self, signal_id: SignalId) {
    self.slots.borrow_mut().free(signal_id.0);
  }

  pub fn contains(&self, signal_id: SignalId) -> bool {
    self.slots.borrow().contains(signal_id.0)
  }

  pub fn count(&self) -> usize {
    self.slots.borrow().len()
  }

  fn get_slot(&self, signal_id: SignalId) -> Rc<SignalSlot> {
    let arena = self.slots.borrow();
    Rc::clone(arena.get(signal_id.0).expect("signal slot not found for the specified id"))
  }
}
