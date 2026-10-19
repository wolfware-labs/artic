use crate::Object;
use crate::tree::{NodeId, UiTree};
use std::fmt::{self, Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

/// A [`NodeId`] that knows its node's type, as [`UiTree::alloc_typed`] and generated `build`
/// methods return it.
///
/// It is `Copy` and the size of a `NodeId`. Every method needs the tree, so a handle can be kept
/// and sent anywhere a `NodeId` can. Using a handle whose node was freed is a bug and panics, as
/// disposed signals do; [`try_get`](Handle::try_get) is for code that expects nodes to go away.
pub struct Handle<T> {
  id: NodeId,
  node_type: PhantomData<fn() -> T>,
}

impl<T> Handle<T> {
  pub(super) const fn new(id: NodeId) -> Self {
    Self {
      id,
      node_type: PhantomData,
    }
  }

  /// The untyped id.
  #[must_use]
  pub const fn id(self) -> NodeId {
    self.id
  }
}

impl<T: Object> Handle<T> {
  /// The node.
  ///
  /// # Panics
  ///
  /// If the node was freed.
  #[must_use]
  pub fn get(self, tree: &UiTree) -> &T {
    self.try_get(tree).expect("the handle's node was freed")
  }

  /// The node, mutably.
  ///
  /// # Panics
  ///
  /// If the node was freed.
  #[must_use]
  pub fn get_mut(self, tree: &mut UiTree) -> &mut T {
    tree.get_as_mut(self.id).expect("the handle's node was freed")
  }

  /// The node, or `None` if it was freed.
  #[must_use]
  pub fn try_get(self, tree: &UiTree) -> Option<&T> {
    tree.get_as(self.id)
  }
}

impl<T> Clone for Handle<T> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<T> Copy for Handle<T> {}

impl<T> PartialEq for Handle<T> {
  fn eq(&self, other: &Self) -> bool {
    self.id == other.id
  }
}

impl<T> Eq for Handle<T> {}

impl<T> Hash for Handle<T> {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.id.hash(state);
  }
}

impl<T> Debug for Handle<T> {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    formatter.debug_tuple("Handle").field(&self.id).finish()
  }
}

impl<T> From<Handle<T>> for NodeId {
  fn from(handle: Handle<T>) -> NodeId {
    handle.id
  }
}
