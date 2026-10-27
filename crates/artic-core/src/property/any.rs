use crate::property::state::PropertyState;
use crate::property::{Property, PropertyKey};
use crate::reactive::Change;
use crate::tree::DirtyHandle;
use std::any::Any;
use std::fmt::{self, Debug, Formatter};

/// The identity of a property kind: the address of its [`PropertyKey`] static.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub(crate) struct KeyId(*const ());

impl KeyId {
  /// Returns the identity of `key`.
  pub(crate) fn of<T>(key: &'static PropertyKey<T>) -> KeyId {
    KeyId(std::ptr::from_ref(key).cast())
  }
}

/// What [`Object::visit_properties`](crate::Object::visit_properties) lists a node's properties
/// to: call [`visit`](PropertyVisitor::visit) with each one.
///
/// The tree makes one to connect a node's properties to the node, to find a property by key and
/// to free them with the node. What it does with each property stays inside artic-core.
pub struct PropertyVisitor<'a>(&'a mut (dyn FnMut(AnyProperty<'_>) + 'a));

impl<'a> PropertyVisitor<'a> {
  /// A visitor that calls `action` with each property.
  pub(crate) fn new(action: &'a mut (dyn FnMut(AnyProperty<'_>) + 'a)) -> PropertyVisitor<'a> {
    PropertyVisitor(action)
  }

  /// Lists `property`, one of the node's own.
  ///
  /// # Panics
  ///
  /// When the tree adds the node, if `property` is already listed by another node: a property
  /// belongs to one node, which disposes it when it's freed.
  pub fn visit<T>(&mut self, property: &Property<T>)
  where
    T: PartialEq + 'static,
  {
    (self.0)(AnyProperty(property));
  }
}

impl Debug for PropertyVisitor<'_> {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    formatter.debug_struct("PropertyVisitor").finish_non_exhaustive()
  }
}

/// One of a node's properties without its value type, as the tree sees it through a
/// [`PropertyVisitor`].
#[derive(Clone, Copy)]
pub(crate) struct AnyProperty<'a>(&'a dyn Erased);

impl<'a> AnyProperty<'a> {
  /// Erases the value type of `property`.
  pub(crate) fn new(property: &'a dyn Erased) -> AnyProperty<'a> {
    AnyProperty(property)
  }

  /// The identity of the property's kind.
  pub(crate) fn key_id(self) -> KeyId {
    self.0.key_id()
  }

  /// The property as a `Property<T>`, or `None` if its value isn't a `T`.
  pub(crate) fn downcast<T: 'static>(self) -> Option<&'a Property<T>> {
    self.0.as_any().downcast_ref()
  }

  /// Whether the property's kind inherits from logical ancestors.
  pub(crate) fn inherits(self) -> bool {
    self.0.inherits()
  }

  /// Whether a node in a tree already lists the property.
  ///
  /// # Panics
  ///
  /// If the property was disposed, as any read of it does.
  pub(crate) fn is_listed(self) -> bool {
    self.0.is_listed()
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
/// `Property<T>` is its only implementor and `as_any` returns the property itself, which is what
/// [`AnyProperty::downcast`] relies on.
pub(crate) trait Erased {
  fn key_id(&self) -> KeyId;
  fn as_any(&self) -> &dyn Any;
  fn inherits(&self) -> bool;
  fn is_listed(&self) -> bool;
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

  fn is_listed(&self) -> bool {
    self.0.with_untracked(PropertyState::is_listed)
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
