use crate::property::{Property, PropertyKey};
use crate::reactive::Change;
use crate::tree::DirtyHandle;
use std::any::Any;
use std::fmt::{self, Debug, Formatter};

/// The identity of a property kind: the address of its [`PropertyKey`] static.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct KeyId(*const ());

impl KeyId {
  /// Returns the identity of `key`.
  #[must_use]
  pub fn of<T>(key: &'static PropertyKey<T>) -> KeyId {
    KeyId(std::ptr::from_ref(key).cast())
  }
}

/// One of a node's properties without its value type, as
/// [`Object::visit_properties`](crate::Object::visit_properties) hands it out. Make one with
/// [`Property::as_ref`].
///
/// The tree uses it to connect a node's properties to the node and to find a property by key. The
/// hooks it does that with are private to artic-core, so no other crate can unplug a node from its
/// dirty flags:
///
/// ```compile_fail,E0599
/// use artic_core::property::{Affects, Inheritance, Property, PropertyKey};
///
/// static WIDTH: PropertyKey<f64> = PropertyKey::new("Width", Affects::LAYOUT, Inheritance::None);
///
/// let width = Property::new(&WIDTH, 0.0);
/// width.as_ref().dispose();
/// ```
#[derive(Clone, Copy)]
pub struct AnyProperty<'a>(&'a dyn Erased);

impl<'a> AnyProperty<'a> {
  /// Erases the value type of `property`.
  pub(crate) fn new(property: &'a dyn Erased) -> AnyProperty<'a> {
    AnyProperty(property)
  }

  /// The identity of the property's kind.
  #[must_use]
  pub fn key_id(self) -> KeyId {
    self.0.key_id()
  }

  /// The property as a `Property<T>`, or `None` if its value isn't a `T`.
  #[must_use]
  pub fn downcast<T: 'static>(self) -> Option<&'a Property<T>> {
    self.0.as_any().downcast_ref()
  }

  /// Whether the property's kind inherits from logical ancestors.
  pub(crate) fn inherits(self) -> bool {
    self.0.inherits()
  }

  /// Connects the property to its node's dirty flags. After this, a change to the effective value
  /// adds the key's [`Affects`](super::Affects) to them and sets the tree's pending flag.
  pub(crate) fn attach(self, dirty: DirtyHandle) {
    self.0.attach(dirty);
  }

  /// Makes the property's `Inherited` level follow `source`, a property of the same kind on a
  /// logical ancestor, or stop following anything when `source` is `None`. The tree calls it when
  /// the node moves; a property of another kind is ignored.
  pub(crate) fn inherit_from(self, source: Option<AnyProperty<'_>>) {
    self.0.inherit_from(source);
  }

  /// Frees the property's reactive state. The tree calls it when it frees the property's node, after
  /// which the property must not be read.
  pub(crate) fn dispose(self) {
    self.0.dispose();
  }
}

impl Debug for AnyProperty<'_> {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    Debug::fmt(self.0, formatter)
  }
}

impl Debug for dyn Erased + '_ {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    formatter.debug_tuple("AnyProperty").field(&self.key_id()).finish()
  }
}

/// A [`Property`] without its value type: what [`AnyProperty`] points at, and what the tree stores
/// a node's attached values as. Each method backs the `AnyProperty` method of the same name.
pub(crate) trait Erased {
  fn key_id(&self) -> KeyId;
  fn as_any(&self) -> &dyn Any;
  fn inherits(&self) -> bool;
  fn attach(&self, dirty: DirtyHandle);
  fn inherit_from(&self, source: Option<AnyProperty<'_>>);
  fn dispose(&self);
}

impl<T> Erased for Property<T>
where
  T: PartialEq + 'static,
{
  fn key_id(&self) -> KeyId {
    KeyId::of(self.key())
  }

  fn as_any(&self) -> &dyn Any {
    self
  }

  fn inherits(&self) -> bool {
    self.key().inherits()
  }

  fn attach(&self, dirty: DirtyHandle) {
    self.0.update_with(|state| {
      state.attach(dirty);
      Change::Hidden
    });
  }

  fn inherit_from(&self, source: Option<AnyProperty<'_>>) {
    self.follow(source.and_then(AnyProperty::downcast).copied());
  }

  fn dispose(&self) {
    Property::dispose(*self);
  }
}
