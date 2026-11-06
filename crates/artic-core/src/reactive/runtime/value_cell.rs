use std::any::Any;
use std::cell::{RefCell, RefMut};
use std::rc::Rc;

/// A signal's or a memo's value, in one allocation with the borrow that guards it.
pub struct ValueCell<TValue: ?Sized> {
  kind: &'static str,
  value: RefCell<TValue>,
}

impl ValueCell<dyn Any> {
  pub fn new<TValue: Any>(value: TValue, kind: &'static str) -> Rc<Self> {
    Rc::new(ValueCell {
      kind,
      value: RefCell::new(value),
    })
  }

  pub fn with<TValue, TResult>(&self, action: impl FnOnce(&TValue) -> TResult) -> TResult
  where
    TValue: Any,
  {
    let Ok(value) = self.value.try_borrow() else {
      panic!("{} read while it is being updated", self.kind);
    };
    action(self.downcast(value.downcast_ref::<TValue>()))
  }

  /// Borrows the value for a write, which [`Taken::write`] makes. Panics, before anything
  /// changes, if the value is being read or updated.
  pub fn take(&self) -> Taken<'_> {
    let Ok(value) = self.value.try_borrow_mut() else {
      panic!("{} written while it is being read or updated", self.kind);
    };
    Taken { cell: self, value }
  }

  fn downcast<TValue>(&self, value: Option<TValue>) -> TValue {
    value.unwrap_or_else(|| panic!("{} type does not match with the specified type", self.kind))
  }
}

/// A value borrowed from its cell for a write, given back when the write ends or unwinds.
pub struct Taken<'a> {
  cell: &'a ValueCell<dyn Any>,
  value: RefMut<'a, dyn Any>,
}

impl Taken<'_> {
  /// Calls `action` with the value.
  pub fn write<TValue, TResult>(mut self, action: impl FnOnce(&mut TValue) -> TResult) -> TResult
  where
    TValue: Any,
  {
    let value = self.value.downcast_mut::<TValue>();
    action(self.cell.downcast(value))
  }
}
