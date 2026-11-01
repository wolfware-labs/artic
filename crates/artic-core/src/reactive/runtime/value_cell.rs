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

  pub fn with_mut<TValue, TResult>(&self, action: impl FnOnce(&mut TValue) -> TResult) -> TResult
  where
    TValue: Any,
  {
    let taken = self.value.try_borrow_mut().ok().and_then(|mut value| value.take());
    let Some(value) = taken else {
      panic!("{} written while it is being read or updated", self.kind);
    };
    let mut taken = Taken {
      cell: &self.value,
      value: Some(value),
    };
    action(self.downcast(taken.value.as_mut().and_then(|value| value.downcast_mut::<TValue>())))
  }

  fn downcast<TValue>(&self, value: Option<TValue>) -> TValue {
    value.unwrap_or_else(|| panic!("{} type does not match with the specified type", self.kind))
  }
}

/// A value taken out of its cell for a write, put back when the write ends or unwinds.
struct Taken<'a> {
  cell: &'a RefCell<Option<Box<dyn Any>>>,
  value: Option<Box<dyn Any>>,
}

impl Drop for Taken<'_> {
  fn drop(&mut self) {
    self.cell.replace(self.value.take());
  }
}
