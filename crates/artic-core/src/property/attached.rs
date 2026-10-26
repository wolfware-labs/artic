use crate::property::{Affects, Inheritance, PropertyKey};
use std::fmt::{Debug, Formatter, Result};

/// The key of an attached property: a property kind one type declares and other nodes hold, such as
/// `Grid.Row` on the children of a grid.
///
/// A node doesn't have a field for it; [`UiTree::attached_property`] creates the property on
/// the node the first time it's needed, starting at the key's default, and
/// [`UiTree::attached`] reads the default without creating anything. Declare one `static` per
/// attached property, for the same reason as a [`PropertyKey`].
///
/// An inheriting attached property follows the nearest logical ancestor that holds it. A node that
/// doesn't hold it reads that ancestor's value through [`UiTree::attached`]; one that needs to
/// learn of changes, as a dirty flag, holds it through [`UiTree::attached_property`].
///
/// [`UiTree::attached`]: crate::tree::UiTree::attached
/// [`UiTree::attached_property`]: crate::tree::UiTree::attached_property
///
/// # Examples
///
/// ```
/// use artic_core::property::{Affects, AttachedKey, Inheritance};
///
/// static ROW: AttachedKey<usize> = AttachedKey::new("Row", Affects::LAYOUT, Inheritance::None, || 0);
///
/// assert_eq!(ROW.key().name(), "Row");
/// assert_eq!(ROW.default_value(), 0);
/// ```
///
/// A `const` key doesn't compile where a key is used, as a [`PropertyKey`] doesn't:
///
/// ```compile_fail,E0716
/// use artic_core::property::{Affects, AttachedKey, Inheritance};
///
/// const ROW: AttachedKey<usize> = AttachedKey::new("Row", Affects::LAYOUT, Inheritance::None, || 0);
///
/// let key: &'static AttachedKey<usize> = &ROW;
/// ```
pub struct AttachedKey<T> {
  key: PropertyKey<T>,
  default: fn() -> T,
}

impl<T> AttachedKey<T> {
  /// Creates the key for an attached property named `name`, whose changes redo `affects`, which
  /// takes part in inheritance as `inheritance` says, and whose value is `default()` on a node that
  /// neither holds it nor inherits it.
  #[must_use]
  pub const fn new(name: &'static str, affects: Affects, inheritance: Inheritance<T>, default: fn() -> T) -> Self {
    Self {
      key: PropertyKey::new(name, affects, inheritance),
      default,
    }
  }

  /// Returns the property key the attached property is stored under.
  #[must_use]
  pub const fn key(&self) -> &PropertyKey<T> {
    &self.key
  }

  /// Returns the value of a node that doesn't set the property.
  #[must_use]
  pub fn default_value(&self) -> T {
    (self.default)()
  }
}

impl<T> Debug for AttachedKey<T> {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    formatter
      .debug_struct("AttachedKey")
      .field("key", &self.key)
      .finish_non_exhaustive()
  }
}
