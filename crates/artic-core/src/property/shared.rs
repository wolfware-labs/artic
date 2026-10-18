//! What a field needs from a key it shares with other types: where it's stored, and the value it
//! starts at.
//!
//! A field can share a [`PropertyKey`] or an attached property's [`AttachedKey`]. Both have a
//! `key()` that returns the property key the value is stored under, which const code can call.
//! [`SharedKey`] gives the starting value: an attached key's own default, or the value type's
//! `Default`.

use super::{AttachedKey, PropertyKey};

/// A key a field can share: a property key, or an attached property's key.
pub trait SharedKey<T> {
  /// The value a field that shares this key starts at when it names no default.
  fn initial_value(&self) -> T;
}

impl<T: Default> SharedKey<T> for PropertyKey<T> {
  fn initial_value(&self) -> T {
    T::default()
  }
}

impl<T> SharedKey<T> for AttachedKey<T> {
  fn initial_value(&self) -> T {
    self.default_value()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::property::{Affects, Inheritance};

  static SIZE: PropertyKey<f32> = PropertyKey::new("Size", Affects::LAYOUT, Inheritance::None);
  static ROW: AttachedKey<f32> = AttachedKey::new("Row", Affects::LAYOUT, Inheritance::None, || 14.0);
  const COLUMN: AttachedKey<f32> = AttachedKey::new("Column", Affects::LAYOUT, Inheritance::None, || 0.0);

  // A property key starts a field at the type's default; an attached key at its own.
  #[test]
  fn initial_value_comes_from_the_key() {
    assert_eq!(SIZE.initial_value(), 0.0);
    assert_eq!(ROW.initial_value(), 14.0);
  }

  // Both kinds read the property key they store under through `key()`, in const code too.
  #[test]
  fn both_kinds_give_their_property_key() {
    const NAME: &str = COLUMN.key().name();

    assert_eq!(SIZE.key().name(), "Size");
    assert_eq!(ROW.key().name(), "Row");
    assert_eq!(NAME, "Column");
    assert!(std::ptr::eq(SIZE.key(), &SIZE));
  }
}
