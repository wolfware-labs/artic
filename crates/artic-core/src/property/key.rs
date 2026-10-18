use crate::property::Affects;
use std::fmt::{Debug, Formatter, Result};
use std::marker::PhantomData;

/// The identity and metadata of one kind of property, shared by every node type that has it.
///
/// Declare one `static` per property kind. Two properties are the same kind when they point at the
/// same `static`, even on different node types: a `FontSize` on a window and a `FontSize` on a text
/// block both use the same key, which is how one inherits from the other. The key also fixes the
/// value type, so a lookup by key can't return a property of another type.
///
/// It must be a `static`, not a `const`: a `const` is copied into each place that uses it and has no
/// single address to compare.
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
pub struct PropertyKey<T> {
  name: &'static str,
  affects: Affects,
  inheritance: Inheritance<T>,
  value_type: PhantomData<fn() -> T>,
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
