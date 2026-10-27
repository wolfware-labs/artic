use crate::property::Affects;
use std::fmt::{Debug, Formatter, Result};
use std::marker::PhantomData;
use std::sync::atomic::AtomicBool;

/// The identity and metadata of one kind of property, shared by every node type that has it.
///
/// Declare one `static` per property kind. Two properties are the same kind when they point at the
/// same `static`, even on different node types: a `FontSize` on a window and a `FontSize` on a text
/// block both use the same key, which is how one inherits from the other. The key also fixes the
/// value type, so a lookup by key can't return a property of another type.
///
/// It must be a `static`: a `const` is copied into each place that uses it and has no single
/// address to compare, so a value set through one copy would be lost to the others. A `const` key
/// doesn't compile where a key is used. For the same reason, a `const` can't read a static key's
/// metadata, such as `const N: &str = KEY.name()`; a `static` initializer and running code can.
///
/// # Examples
///
/// ```
/// use artic_core::property::{Affects, Inheritance, PropertyKey};
///
/// static FONT_SIZE: PropertyKey<f32> = PropertyKey::new("FontSize", Affects::LAYOUT, Inheritance::Replace);
///
/// assert_eq!(FONT_SIZE.name(), "FontSize");
/// assert!(FONT_SIZE.inherits());
/// ```
///
/// A `const` key doesn't compile where a key is used:
///
/// ```compile_fail,E0716
/// use artic_core::property::{Affects, Inheritance, PropertyKey};
///
/// const FONT_SIZE: PropertyKey<f32> = PropertyKey::new("FontSize", Affects::LAYOUT, Inheritance::Replace);
///
/// let key: &'static PropertyKey<f32> = &FONT_SIZE;
/// ```
pub struct PropertyKey<T> {
  name: &'static str,
  affects: Affects,
  inheritance: Inheritance<T>,
  value_type: PhantomData<fn() -> T>,
  /// Never read: a field with interior mutability makes the key `!Freeze`, so Rust doesn't
  /// promote a `const` key to `&'static`, and a `const` key used as a key doesn't compile.
  #[expect(dead_code, reason = "its type, not its value, does the work")]
  not_const: AtomicBool,
}

/// How a property takes part in inheritance from its logical ancestors.
pub enum Inheritance<T> {
  /// It doesn't inherit.
  None,
  /// Its own value, when one is set above the `Inherited` level, replaces what it inherits;
  /// otherwise it takes the nearest ancestor's effective value.
  Replace,
  /// Its effective value is the function of its own value, from its levels or its default, and
  /// the nearest ancestor's effective value, such as [`inherit::and`](super::inherit::and) for a
  /// flag every ancestor must allow.
  Combine(fn(&T, &T) -> T),
}

impl<T> Inheritance<T> {
  /// Calls `function` with the effective value of a property of this kind, from its own value
  /// (its highest set level, if any), its default and what it inherits from its nearest logical
  /// ancestor, if any. Only a kind that inherits is ever given an inherited value.
  pub(crate) fn resolve<R>(self, own: Option<&T>, default: &T, inherited: Option<&T>, function: impl FnOnce(&T) -> R) -> R {
    match (self, inherited) {
      (Inheritance::Combine(combine), Some(inherited)) => function(&combine(own.unwrap_or(default), inherited)),
      (_, inherited) => function(own.or(inherited).unwrap_or(default)),
    }
  }
}

impl<T> Clone for Inheritance<T> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<T> Copy for Inheritance<T> {}

impl<T> Debug for Inheritance<T> {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    match self {
      Inheritance::None => formatter.write_str("None"),
      Inheritance::Replace => formatter.write_str("Replace"),
      Inheritance::Combine(_) => formatter.write_str("Combine"),
    }
  }
}

impl<T> PropertyKey<T> {
  /// Creates the key for a property kind named `name`.
  ///
  /// `affects` lists the frame stages a change to the property's effective value has to redo.
  /// `inheritance` says whether the property takes part in inheritance from the nearest logical
  /// ancestor that has the same key, and how.
  #[must_use]
  pub const fn new(name: &'static str, affects: Affects, inheritance: Inheritance<T>) -> Self {
    Self {
      name,
      affects,
      inheritance,
      value_type: PhantomData,
      not_const: AtomicBool::new(false),
    }
  }

  /// Returns this key: the property key a value is stored under.
  /// [`AttachedKey::key`](super::AttachedKey::key) returns the one an attached property is stored
  /// under, so code that takes either kind reads it the same way.
  #[must_use]
  pub const fn key(&self) -> &PropertyKey<T> {
    self
  }

  /// The property's name, as UI languages spell it.
  #[must_use]
  pub const fn name(&self) -> &'static str {
    self.name
  }

  /// The frame stages a change to the property's effective value has to redo.
  #[must_use]
  pub const fn affects(&self) -> Affects {
    self.affects
  }

  /// Whether the property takes part in inheritance, by replacing or by combining.
  #[must_use]
  pub const fn inherits(&self) -> bool {
    !matches!(self.inheritance, Inheritance::None)
  }

  /// How the property takes part in inheritance.
  #[must_use]
  pub const fn inheritance(&self) -> Inheritance<T> {
    self.inheritance
  }
}

impl<T> Debug for PropertyKey<T> {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    formatter
      .debug_struct("PropertyKey")
      .field("name", &self.name)
      .field("affects", &self.affects)
      .field("inheritance", &self.inheritance)
      .finish()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::property::inherit;
  use pretty_assertions::assert_eq;

  // Own value, default and inherited value fold one way for every reader: `Replace` takes the own
  // value, else the inherited one, else the default; `Combine` applies its function to the own
  // value or default and the inherited one.
  #[test]
  fn resolve_folds_own_default_and_inherited() {
    let replace = |own, inherited| Inheritance::<i32>::Replace.resolve(own, &0, inherited, |value| *value);
    let and = |own, inherited| Inheritance::Combine(inherit::and).resolve(own, &true, inherited, |value| *value);

    assert_eq!(
      [replace(Some(&3), Some(&7)), replace(None, Some(&7)), replace(None, None)],
      [3, 7, 0]
    );
    assert_eq!(
      [
        and(Some(&true), Some(&false)),
        and(None, Some(&true)),
        and(Some(&false), None),
        and(None, None)
      ],
      [false, true, false, true]
    );
  }
}
