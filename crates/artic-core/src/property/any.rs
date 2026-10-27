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

/// A [`Property`] seen without its value type, as [`Object::visit_properties`] hands it out.
///
/// The tree uses it to connect a node's properties to the node and to find a property by key.
/// Every `Property<T>` implements it.
///
/// [`Object::visit_properties`]: crate::Object::visit_properties
pub trait AnyProperty {
  /// The identity of the property's kind.
  fn key_id(&self) -> KeyId;

  /// Whether the property's kind inherits from logical ancestors.
  fn inherits(&self) -> bool;

  /// Connects the property to its node's dirty flags. After this, a change to the effective value
  /// adds the key's [`Affects`](super::Affects) to them and sets the tree's pending flag.
  fn attach(&self, dirty: DirtyHandle);

  /// Makes the property's `Inherited` level follow `source`, a property of the same kind on a
  /// logical ancestor, or stop following anything when `source` is `None`. The tree calls it when
  /// the node moves; a property of another kind is ignored.
  fn inherit_from(&self, source: Option<&dyn AnyProperty>);

  /// Frees the property's reactive state. The tree calls it when it frees the property's node, after
  /// which the property must not be read.
  fn dispose(&self);

  /// The property as `Any`, to get the typed `Property<T>` back.
  fn as_any(&self) -> &dyn Any;
}

impl Debug for dyn AnyProperty {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    formatter.debug_tuple("AnyProperty").field(&self.key_id()).finish()
  }
}

impl<T> AnyProperty for Property<T>
where
  T: PartialEq + 'static,
{
  fn key_id(&self) -> KeyId {
    KeyId::of(self.key())
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

  fn inherit_from(&self, source: Option<&dyn AnyProperty>) {
    let source = source.and_then(|source| source.as_any().downcast_ref::<Property<T>>());
    self.follow(source.copied());
  }

  fn dispose(&self) {
    Property::dispose(*self);
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}
