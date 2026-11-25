//! Values written as elements: types whose objects a UI language stages in the tree while it sets
//! their members, then moves into the property that holds them
//! ([value objects design](https://github.com/wolfware-labs/artic/blob/main/docs/design/value-objects.md)).

use crate::{UiCollection, UiType, ValueError};
use artic_core::Object;
use artic_core::tree::{NodeId, TreeKind, UiTree};
use std::sync::Arc;

/// A UI type that is a value: immutable data a property holds, such as a brush, copied into each
/// place that uses it. `#[derive(UiType)]` with `#[ui(value)]` implements it.
///
/// Its object is staged in the tree only while a UI language sets its members; it never stays
/// there. Its fields are plain data, not properties, so taking it out of the tree loses nothing.
pub trait UiValue: UiType + Object + Clone {}

/// A property value type that can be taken from an object a UI language staged in the tree.
///
/// Every [`UiValue`] takes itself. A family of values takes its members, as `Brush` takes each
/// kind of brush through its `From` impls, so a markup element and Rust code convert the same way.
pub trait FromUiObject: Sized {
  /// Takes the staged `object` out of the tree as `Self`. The object is taken out whether or not it
  /// converts, so no staged value is left behind.
  ///
  /// # Panics
  ///
  /// If `object` isn't in the tree.
  ///
  /// # Errors
  ///
  /// [`ValueError::WrongType`] when the object is of a type `Self` doesn't take.
  fn from_object(tree: &mut UiTree, object: NodeId) -> Result<Self, ValueError>;
}

/// What a value converted from an object of another type says it was given.
pub const ANOTHER_OBJECT: &str = "an object of another type";

impl<T: UiValue> FromUiObject for T {
  fn from_object(tree: &mut UiTree, object: NodeId) -> Result<T, ValueError> {
    tree
      .take(object)
      .downcast::<T>()
      .map(|value| *value)
      .map_err(|_| ValueError::WrongType {
        expected: T::TYPE_INFO.name,
        found: ANOTHER_OBJECT,
      })
  }
}

/// A collection of values, in the order they're written.
impl<T: FromUiObject> UiCollection for Vec<T> {
  fn push_child(
    tree: &mut UiTree,
    owner: NodeId,
    child: NodeId,
    field: for<'a> fn(&'a mut UiTree, NodeId) -> Option<&'a mut Self>,
  ) -> Result<(), ValueError> {
    push_value(tree, owner, child, field, Vec::push)
  }

  /// Takes the staged child out first, so it checks the owner after that.
  fn add_child(
    tree: &mut UiTree,
    owner: NodeId,
    child: NodeId,
    field: for<'a> fn(&'a mut UiTree, NodeId) -> Option<&'a mut Self>,
  ) -> Result<(), ValueError> {
    Self::push_child(tree, owner, child, field)
  }
}

/// A shared collection of values, such as a gradient's stops, which a brush clones cheaply into
/// each draw. Each value written makes a new slice one longer; a collection written in markup is
/// short, and only a UI language's loader fills one this way.
impl<T: FromUiObject + Clone> UiCollection for Arc<[T]> {
  fn push_child(
    tree: &mut UiTree,
    owner: NodeId,
    child: NodeId,
    field: for<'a> fn(&'a mut UiTree, NodeId) -> Option<&'a mut Self>,
  ) -> Result<(), ValueError> {
    push_value(tree, owner, child, field, |values, value| {
      *values = values.iter().cloned().chain([value]).collect();
    })
  }

  /// Takes the staged child out first, so it checks the owner after that.
  fn add_child(
    tree: &mut UiTree,
    owner: NodeId,
    child: NodeId,
    field: for<'a> fn(&'a mut UiTree, NodeId) -> Option<&'a mut Self>,
  ) -> Result<(), ValueError> {
    Self::push_child(tree, owner, child, field)
  }
}

/// Refuses an `object` that isn't a staged value for `owner`: a stale id, the owner itself, or a
/// node linked in either tree. A staged value is a detached leaf, so taking it out of the tree
/// frees nothing else.
pub(crate) fn check_staged(tree: &UiTree, owner: NodeId, object: NodeId) -> Result<(), ValueError> {
  let linked = TreeKind::ALL
    .into_iter()
    .any(|kind| tree.parent(kind, object).is_some() || tree.children(kind, object).next().is_some());
  if tree.contains(object) && object != owner && !linked {
    Ok(())
  } else {
    Err(ValueError::NotStaged(object))
  }
}

/// Takes the staged `child` out of the tree as a `T`, then gives it to `push` with the collection
/// `field` reaches on `owner`: the steps every collection of values shares.
///
/// A child that isn't a staged value is refused with the tree untouched. Otherwise the child is
/// taken out whatever happens next, as a property's `set_object` takes its object, so a wrong
/// owner or a value of another type leaves nothing staged.
fn push_value<C, T: FromUiObject>(
  tree: &mut UiTree,
  owner: NodeId,
  child: NodeId,
  field: for<'a> fn(&'a mut UiTree, NodeId) -> Option<&'a mut C>,
  push: impl FnOnce(&mut C, T),
) -> Result<(), ValueError> {
  check_staged(tree, owner, child)?;
  let value = T::from_object(tree, child)?;
  push(field(tree, owner).ok_or(ValueError::WrongNode)?, value);
  Ok(())
}
