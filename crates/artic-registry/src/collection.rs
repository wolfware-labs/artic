//! The fields that receive an element's child elements.
//!
//! A `#[ui]` field that is neither a `Property` nor an `Event` is a collection: when markup writes
//! elements inside the type's element, each child is added to it, in order, through
//! [`UiCollection`].

use crate::ValueError;
use artic_core::tree::{NodeId, UiTree};

/// A field that receives the child elements written inside an element, in order.
///
/// A panel's children are the usual example: `<StackPanel><A/><B/></StackPanel>` adds `A`, then
/// `B`. A collection may keep what it receives in the tree, as a panel's children are, or in its
/// field, as a brush keeps its gradient stops; `field` reaches the field inside the owner node.
#[diagnostic::on_unimplemented(
  message = "`{Self}` can't hold child elements",
  label = "`#[ui]` on a field that isn't a `Property` or an `Event` makes it a collection of children",
  note = "to make this field a property, declare it as `Property<{Self}>`"
)]
pub trait UiCollection: Sized {
  /// Adds `child` as content of `owner`. An implementation calls `field` first and returns
  /// [`ValueError::WrongNode`] when it gives `None`, before changing anything.
  ///
  /// # Errors
  ///
  /// When `owner` isn't the field's owner type, or the collection refuses `child`.
  fn push_child(
    tree: &mut UiTree,
    owner: NodeId,
    child: NodeId,
    field: for<'a> fn(&'a mut UiTree, NodeId) -> Option<&'a mut Self>,
  ) -> Result<(), ValueError>;
}
