use super::nodes;
use crate::tree::{LinkError, NodeId, TreeKind, UiTree};
use pretty_assertions::assert_eq;

/// The logical and the visual children of `parent`.
fn both(tree: &UiTree, parent: NodeId) -> [Vec<NodeId>; 2] {
  TreeKind::ALL.map(|kind| tree.children(kind, parent).collect())
}

// `link_both` makes the child the same child in both trees, at the end or at an index.
#[test]
fn link_both_links_in_both_trees() {
  let mut tree = UiTree::new();
  let [panel, a, b] = nodes(&mut tree);

  assert_eq!(tree.link_both(panel, None, a), Ok(()));
  assert_eq!(tree.link_both(panel, Some(0), b), Ok(()));

  assert_eq!(both(&tree, panel), [vec![b, a], vec![b, a]]);
}

// A child under another parent in one tree leaves that parent and joins the new one in both trees.
#[test]
fn link_both_moves_a_child_from_any_parent() {
  let mut tree = UiTree::new();
  let [old, panel, child] = nodes(&mut tree);
  tree.add_child(TreeKind::Visual, old, child);

  assert_eq!(tree.link_both(panel, None, child), Ok(()));

  assert_eq!(both(&tree, panel), [vec![child], vec![child]]);
  assert_eq!(both(&tree, old), [vec![], vec![]]);
}

// Moving the last child to the end keeps one copy in each list.
#[test]
fn link_both_moves_a_child_in_place() {
  let mut tree = UiTree::new();
  let [panel, a, b] = nodes(&mut tree);
  tree.link_both(panel, None, a).unwrap();
  tree.link_both(panel, None, b).unwrap();

  assert_eq!(tree.link_both(panel, None, b), Ok(()));
  assert_eq!(tree.link_both(panel, Some(1), a), Ok(()));

  assert_eq!(both(&tree, panel), [vec![b, a], vec![b, a]]);
}

// A link one tree refuses changes neither tree: a cycle in the visual tree only.
#[test]
fn link_both_checks_both_trees_first() {
  let mut tree = UiTree::new();
  let [panel, child] = nodes(&mut tree);
  tree.add_child(TreeKind::Visual, child, panel);

  assert_eq!(
    tree.link_both(panel, None, child),
    Err(LinkError::Cycle {
      tree: TreeKind::Visual,
      parent: panel,
      child,
    })
  );

  assert_eq!(both(&tree, panel), [vec![], vec![]]);
  assert_eq!(tree.parent(TreeKind::Logical, child), None);
}

// A parent whose two child lists differ refuses the link and changes nothing.
#[test]
fn link_both_refuses_children_that_differ() {
  let mut tree = UiTree::new();
  let [panel, a, b] = nodes(&mut tree);
  tree.add_child(TreeKind::Visual, panel, a);

  assert_eq!(
    tree.link_both(panel, None, b),
    Err(LinkError::ChildrenDiffer { parent: panel })
  );

  assert_eq!(both(&tree, panel), [vec![], vec![a]]);
}

// `unlink_both` detaches the child from the parent in both trees; the node stays in the tree.
#[test]
fn unlink_both_detaches_in_both_trees() {
  let mut tree = UiTree::new();
  let [panel, child] = nodes(&mut tree);
  tree.link_both(panel, None, child).unwrap();

  assert_eq!(tree.unlink_both(panel, child), Ok(()));

  assert_eq!(both(&tree, panel), [vec![], vec![]]);
  assert!(tree.contains(child));
}

// A node that isn't the parent's child is refused, and its own parent keeps it.
#[test]
fn unlink_both_refuses_a_node_that_is_not_the_child() {
  let mut tree = UiTree::new();
  let [panel, other, child] = nodes(&mut tree);
  tree.link_both(other, None, child).unwrap();

  assert_eq!(
    tree.unlink_both(panel, child),
    Err(LinkError::NotAChild { parent: panel, child })
  );

  assert_eq!(both(&tree, other), [vec![child], vec![child]]);
}

// `shared_children` is the list both trees agree on, or the reason they don't.
#[test]
fn shared_children_is_the_agreed_list() {
  let mut tree = UiTree::new();
  let [panel, a, b] = nodes(&mut tree);
  tree.link_both(panel, None, a).unwrap();

  assert_eq!(tree.shared_children(panel), Ok(vec![a]));

  tree.add_child(TreeKind::Logical, panel, b);
  assert_eq!(tree.shared_children(panel), Err(LinkError::ChildrenDiffer { parent: panel }));
}
