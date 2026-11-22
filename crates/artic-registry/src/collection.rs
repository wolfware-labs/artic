//! The fields that receive an element's child elements.
//!
//! A type's content field that isn't a `Property` is a collection: when a UI language gives the
//! type child elements, each child is added to it, in order, through [`UiCollection`].

use crate::{TypeInfo, ValueError};
use artic_core::Object;
use artic_core::tree::{NodeId, UiTree};
use std::any::Any;

/// A field that receives the child elements written inside an element, in order.
///
/// A panel's children are the usual example: `<StackPanel><A/><B/></StackPanel>` adds `A`, then
/// `B`. A collection may keep what it receives in the tree, as a panel's children are, or in its
/// field, as a brush keeps its gradient stops; `field` reaches the field inside the owner node.
#[diagnostic::on_unimplemented(
  message = "`{Self}` can't hold child elements",
  label = "a content field that isn't a `Property` is a collection of children",
  note = "to make this field a property, declare it as `Property<{Self}>`"
)]
pub trait UiCollection: Sized {
  /// The types of child the collection takes, or `None` for any. A UI language reports a child of
  /// another type where it's written, and `add_child` and `WithChildren::build` refuse it with
  /// [`ValueError::UnexpectedChild`] before changing anything. It's a function rather than a
  /// constant so a type can list itself, as an inline that holds inlines does.
  #[must_use]
  fn takes() -> Option<&'static [&'static TypeInfo]> {
    None
  }

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

/// Whether `object` is a `T`: what a derived [`TypeInfo::is`] calls.
#[must_use]
pub fn is_a<T: Object>(object: &dyn Object) -> bool {
  (object as &dyn Any).is::<T>()
}

/// Checks that `child` is of a type `takes` lists, when it lists any. A node that isn't in the
/// tree passes, for the link to report.
///
/// # Errors
///
/// [`ValueError::UnexpectedChild`], naming the types, when `child` is of none of them.
pub fn check_child(takes: Option<&[&TypeInfo]>, tree: &UiTree, child: NodeId) -> Result<(), ValueError> {
  match (takes, tree.get(child)) {
    (Some(takes), Some(object)) if !takes.iter().any(|info| (info.is)(object)) => Err(ValueError::UnexpectedChild {
      expected: type_names(takes),
    }),
    _ => Ok(()),
  }
}

/// The names of `types` as a list in prose: "`Run`", "`Run` or `Bold`", "`Run`, `Bold` or `Italic`".
#[must_use]
pub fn type_names(types: &[&TypeInfo]) -> String {
  let names: Vec<String> = types.iter().map(|info| format!("`{}`", info.name)).collect();
  match names.split_last() {
    None => String::new(),
    Some((last, [])) => last.clone(),
    Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::Whitespace;
  use artic_core::property::PropertyVisitor;
  use pretty_assertions::assert_eq;

  #[derive(Debug)]
  struct Leaf;

  impl Object for Leaf {
    fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
  }

  /// A descriptor named `name` that recognizes nodes with `is`, kept for the whole test run.
  fn info(name: &'static str, is: fn(&dyn Object) -> bool) -> &'static TypeInfo {
    Box::leak(Box::new(TypeInfo {
      name,
      rust_path: name,
      is,
      new: None,
      content: None,
      properties: &[],
      groups: &[],
      events: &[],
      attached_properties: &[],
      value_provider: None,
      whitespace: Whitespace::Collapse,
      code_type_root: false,
    }))
  }

  // One name stands alone, two take "or", and more take commas before the "or".
  #[test]
  fn type_names_read_as_a_list() {
    let [leaf, other, third] = [info("Leaf", is_a::<Leaf>), info("Other", |_| false), info("Third", |_| false)];
    assert_eq!(type_names(&[]), "");
    assert_eq!(type_names(&[leaf]), "`Leaf`");
    assert_eq!(type_names(&[leaf, other]), "`Leaf` or `Other`");
    assert_eq!(type_names(&[leaf, other, third]), "`Leaf`, `Other` or `Third`");
  }

  // A child of a listed type passes, one of another type is refused with the list, and any child
  // passes a collection that lists none, as does a node not in the tree.
  #[test]
  fn check_child_follows_the_list() {
    let [leaf_info, other, third] = [info("Leaf", is_a::<Leaf>), info("Other", |_| false), info("Third", |_| false)];
    let mut tree = UiTree::new();
    let leaf = tree.alloc(Box::new(Leaf));
    let stale = tree.alloc(Box::new(Leaf));
    tree.free(stale);

    assert_eq!(check_child(Some(&[leaf_info]), &tree, leaf), Ok(()));
    assert_eq!(
      check_child(Some(&[other, third]), &tree, leaf),
      Err(ValueError::UnexpectedChild {
        expected: "`Other` or `Third`".to_owned()
      })
    );
    assert_eq!(check_child(None, &tree, leaf), Ok(()));
    assert_eq!(check_child(Some(&[other]), &tree, stale), Ok(()));
  }
}
