use std::any::Any;
use std::cell::RefCell;

pub struct ValueCell {
  value: RefCell<Option<Box<dyn Any>>>,
  kind: &'static str,
}

impl ValueCell {
  pub fn new(value: Box<dyn Any>, kind: &'static str) -> Self {
    Self {
      value: RefCell::new(Some(value)),
      kind,
    }
  }

  pub fn with<TValue, TResult>(&self, action: impl FnOnce(&TValue) -> TResult) -> TResult
  where
    TValue: Any,
  {
    let value = self.value.borrow();
    let Some(value) = &*value else {
      panic!("{} read while it is being updated", self.kind);
    };
    action(self.downcast(value.downcast_ref::<TValue>()))
  }

  /// Takes the value out for a write, which [`Taken::write`] makes. Panics, before anything
  /// changes, if the value is being read or updated.
  pub fn take(&self) -> Taken<'_> {
    let taken = self.value.try_borrow_mut().ok().and_then(|mut value| value.take());
    let Some(value) = taken else {
      panic!("{} written while it is being read or updated", self.kind);
    };
    Taken {
      cell: self,
      value: Some(value),
    }
  }

  fn downcast<TValue>(&self, value: Option<TValue>) -> TValue {
    value.unwrap_or_else(|| panic!("{} type does not match with the specified type", self.kind))
  }
}

/// A value taken out of its cell for a write, put back when the write ends or unwinds.
pub struct Taken<'a> {
  cell: &'a ValueCell,
  value: Option<Box<dyn Any>>,
}

impl Taken<'_> {
  /// Calls `action` with the value, then puts it back, also if `action` panics.
  pub fn write<TValue, TResult>(mut self, action: impl FnOnce(&mut TValue) -> TResult) -> TResult
  where
    TValue: Any,
  {
    let value = self.value.as_mut().and_then(|value| value.downcast_mut::<TValue>());
    action(self.cell.downcast(value))
  }
}

impl Drop for Taken<'_> {
  fn drop(&mut self) {
    self.cell.value.replace(self.value.take());
  }
}
