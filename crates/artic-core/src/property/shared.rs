//! What a field needs from a key it shares with other types: where it's stored, and the value it
//! starts at.
//!
//! A field can share a [`PropertyKey`] or an attached property's [`AttachedKey`]. Both have a
//! `key()` that returns the property key the value is stored under, which const code can call.
//! [`SharedKey`] gives the starting value: an attached key's own default, or the value type's
//! `Default`. A field may name its own default only beside a property key, through
//! [`default_beside`]: an attached key already brings one, and two could disagree.

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

/// A key a field may name its own default beside. Only a property key: an attached key brings
/// its own.
#[diagnostic::on_unimplemented(
  message = "`default` can't go beside an attached key",
  label = "this attached key already gives the field its default",
  note = "remove `default = ...`; the field starts at the attached property's default"
)]
pub trait TakesDefault {}

impl<T> TakesDefault for PropertyKey<T> {}

/// Returns `value`, the default a field names beside `key`, once the compiler has checked that
/// `key` may take one.
pub fn default_beside<K: TakesDefault + ?Sized, T>(key: &K, value: T) -> T {
  let _ = key;
  value
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

  // A default named beside a property key is the field's value.
  #[test]
  fn default_beside_a_property_key_is_kept() {
    assert_eq!(default_beside(&SIZE, 3.0), 3.0);
  }
}
