//! Content the tree owns: a panel's children and a single child, as logical and visual children of
//! the same element.

use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_registry::{UiCollection, ValueError};

/// Marks an element's children, as a UI language writes them: they become its logical and visual children, which the tree
/// holds; the field itself holds nothing.
#[derive(Debug, Default, Clone, Copy)]
pub struct Children;

/// Marks an element's single child, as a UI language writes it: its only logical and visual child.
#[derive(Debug, Default, Clone, Copy)]
pub struct Child;

fn current(tree: &UiTree, parent: NodeId) -> Result<Option<NodeId>, ValueError> {
  match tree.shared_children(parent).map_err(ValueError::InvalidChild)?.as_slice() {
    [] => Ok(None),
    [child] => Ok(Some(*child)),
    _ => Err(ValueError::OneChildOnly),
  }
}

impl Children {
  /// Makes `child` the last child of `parent` in both trees, moving it from any parent it had.
  ///
  /// # Panics
  ///
  /// When either tree refuses the link: a node not in the tree, or a cycle.
  pub fn add(tree: &mut UiTree, parent: NodeId, child: NodeId) {
    if let Err(error) = tree.link_both(parent, None, child) {
      panic!("{error}");
    }
  }

  /// Makes `child` the child of `parent` at `index` in both trees.
  ///
  /// # Panics
  ///
  /// When either tree refuses the link, `index` out of range in either included.
  pub fn insert(tree: &mut UiTree, parent: NodeId, index: usize, child: NodeId) {
    if let Err(error) = tree.link_both(parent, Some(index), child) {
      panic!("{error}");
    }
  }

  /// Detaches `child` from `parent` in both trees; the node stays allocated.
  ///
  /// # Panics
  ///
  /// When `child` isn't `parent`'s child in both trees.
  pub fn remove(tree: &mut UiTree, parent: NodeId, child: NodeId) {
    if let Err(error) = tree.unlink_both(parent, child) {
      panic!("{error}");
    }
  }
}

impl UiCollection for Children {
  fn push_child(
    tree: &mut UiTree,
    owner: NodeId,
    child: NodeId,
    field: for<'a> fn(&'a mut UiTree, NodeId) -> Option<&'a mut Self>,
  ) -> Result<(), ValueError> {
    field(tree, owner).ok_or(ValueError::WrongNode)?;
    tree.link_both(owner, None, child).map_err(ValueError::InvalidChild)
  }
}

impl Child {
  /// The child of `parent`, if it has one.
  #[must_use]
  pub fn get(tree: &UiTree, parent: NodeId) -> Option<NodeId> {
    tree.children(TreeKind::Visual, parent).next()
  }

  /// Replaces the child: checks that both trees hold the same single child, or none, and that
  /// `child` can be linked in both, then links `child` and detaches the old child.
  ///
  /// # Panics
  ///
  /// When a check fails; nothing has changed then.
  pub fn set(tree: &mut UiTree, parent: NodeId, child: Option<NodeId>) {
    let old = match current(tree, parent) {
      Ok(old) => old,
      Err(ValueError::InvalidChild(error)) => panic!("{error}"),
      Err(error) => panic!("{error}"),
    };
    if child == old {
      return;
    }
    if let Some(child) = child
      && let Err(error) = tree.link_both(parent, None, child)
    {
      panic!("{error}");
    }
    if let Some(old) = old {
      tree
        .unlink_both(parent, old)
        .expect("the old child is still a shared child of the parent");
    }
  }
}

impl UiCollection for Child {
  fn push_child(
    tree: &mut UiTree,
    owner: NodeId,
    child: NodeId,
    field: for<'a> fn(&'a mut UiTree, NodeId) -> Option<&'a mut Self>,
  ) -> Result<(), ValueError> {
    field(tree, owner).ok_or(ValueError::WrongNode)?;
    if current(tree, owner)?.is_some() {
      return Err(ValueError::OneChildOnly);
    }
    tree.link_both(owner, None, child).map_err(ValueError::InvalidChild)
  }
}
