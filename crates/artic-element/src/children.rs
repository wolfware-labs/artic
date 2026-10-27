//! Content the tree owns: a panel's children and a single child, as logical and visual children of
//! the same element.

use artic_core::tree::{LinkError, NodeId, TreeKind, UiTree};
use artic_registry::{UiCollection, ValueError};

/// Marks an element's children, as a UI language writes them: they become its logical and visual children, which the tree
/// holds; the field itself holds nothing.
#[derive(Debug, Default, Clone, Copy)]
pub struct Children;

/// Marks an element's single child, as a UI language writes it: its only logical and visual child.
#[derive(Debug, Default, Clone, Copy)]
pub struct Child;

fn agreed(tree: &UiTree, parent: NodeId) -> Result<Vec<NodeId>, LinkError> {
  let [logical, visual] = TreeKind::ALL.map(|kind| tree.children(kind, parent).collect::<Vec<NodeId>>());
  if logical == visual {
    Ok(logical)
  } else {
    Err(LinkError::ChildrenDiffer { parent })
  }
}

fn check_both(tree: &UiTree, parent: NodeId, index: Option<usize>, child: NodeId) -> Result<(), LinkError> {
  agreed(tree, parent)?;
  TreeKind::ALL
    .iter()
    .try_for_each(|kind| tree.check_link(*kind, parent, index, child))
}

fn current(tree: &UiTree, parent: NodeId) -> Result<Option<NodeId>, ValueError> {
  match agreed(tree, parent).map_err(ValueError::InvalidChild)?.as_slice() {
    [] => Ok(None),
    [child] => Ok(Some(*child)),
    _ => Err(ValueError::OneChildOnly),
  }
}

fn check_is_child(tree: &UiTree, parent: NodeId, child: NodeId) -> Result<(), LinkError> {
  TreeKind::ALL
    .iter()
    .try_for_each(|kind| match tree.parent(*kind, child) == Some(parent) {
      true => Ok(()),
      false => Err(LinkError::NotAChild {
        tree: *kind,
        parent,
        child,
      }),
    })
}

impl Children {
  /// Makes `child` the last child of `parent` in both trees, moving it from any parent it had.
  ///
  /// # Panics
  ///
  /// When either tree refuses the link: a node not in the tree, or a cycle.
  pub fn add(tree: &mut UiTree, parent: NodeId, child: NodeId) {
    if let Err(error) = check_both(tree, parent, None, child) {
      panic!("{error}");
    }
    for kind in TreeKind::ALL {
      tree.add_child(kind, parent, child);
    }
  }

  /// Makes `child` the child of `parent` at `index` in both trees.
  ///
  /// # Panics
  ///
  /// When either tree refuses the link, `index` out of range in either included.
  pub fn insert(tree: &mut UiTree, parent: NodeId, index: usize, child: NodeId) {
    if let Err(error) = check_both(tree, parent, Some(index), child) {
      panic!("{error}");
    }
    for kind in TreeKind::ALL {
      tree.insert_child(kind, parent, index, child);
    }
  }

  /// Detaches `child` from `parent` in both trees; the node stays allocated.
  ///
  /// # Panics
  ///
  /// When `child` isn't `parent`'s child in both trees.
  pub fn remove(tree: &mut UiTree, parent: NodeId, child: NodeId) {
    if let Err(error) = agreed(tree, parent).and_then(|_| check_is_child(tree, parent, child)) {
      panic!("{error}");
    }
    for kind in TreeKind::ALL {
      tree.detach(kind, child);
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
    check_both(tree, owner, None, child).map_err(ValueError::InvalidChild)?;
    Children::add(tree, owner, child);
    Ok(())
  }
}

impl Child {
  /// The child of `parent`, if it has one.
  #[must_use]
  pub fn get(tree: &UiTree, parent: NodeId) -> Option<NodeId> {
    tree.children(TreeKind::Visual, parent).next()
  }

  /// Replaces the child: checks that both trees hold the same single child, or none, and that
  /// `child` can be linked in both, then detaches the old child and links `child`.
  ///
  /// # Panics
  ///
  /// When a check fails; nothing has changed then.
  pub fn set(tree: &mut UiTree, parent: NodeId, child: Option<NodeId>) {
    let old = match current(tree, parent) {
      Ok(old) => old,
      Err(error) => panic!("{error}"),
    };
    if let Some(Err(error)) = child.map(|child| check_both(tree, parent, None, child)) {
      panic!("{error}");
    }
    if let Some(old) = old {
      for kind in TreeKind::ALL {
        tree.detach(kind, old);
      }
    }
    if let Some(child) = child {
      for kind in TreeKind::ALL {
        tree.add_child(kind, parent, child);
      }
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
    check_both(tree, owner, None, child).map_err(ValueError::InvalidChild)?;
    Child::set(tree, owner, Some(child));
    Ok(())
  }
}
