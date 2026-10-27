use crate::Object;
use crate::property::{Affects, PropertyVisitor};
use crate::tree::{LinkError, NodeId, TreeKind, UiTree};
use pretty_assertions::assert_eq;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Debug)]
struct Control;

impl Object for Control {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

/// Clears every node's dirty flags.
fn clean(tree: &mut UiTree, ids: &[NodeId]) {
  for id in ids {
    tree.clear_dirty(*id, Affects::LAYOUT | Affects::RENDER);
  }
}

/// Allocates `count` nodes and returns their ids, in order.
fn nodes(tree: &mut UiTree, count: usize) -> Vec<NodeId> {
  (0..count).map(|_| tree.alloc(Box::new(Control))).collect()
}

/// Collects the children of `id` in the `kind` tree.
fn children(tree: &UiTree, kind: TreeKind, id: NodeId) -> Vec<NodeId> {
  tree.children(kind, id).collect()
}

// A node's logical and visual parents are set separately: a template part sits between a control
// and its content in the visual tree only.
#[test]
fn logical_and_visual_parents_are_independent() {
  let mut tree = UiTree::new();
  let [button, border, text] = nodes(&mut tree, 3)[..] else {
    unreachable!()
  };

  tree.add_child(TreeKind::Logical, button, text);
  tree.add_child(TreeKind::Visual, button, border);
  tree.add_child(TreeKind::Visual, border, text);

  assert_eq!(tree.parent(TreeKind::Logical, text), Some(button));
  assert_eq!(tree.parent(TreeKind::Visual, text), Some(border));
  assert_eq!(tree.parent(TreeKind::Logical, border), None);
  assert_eq!(children(&tree, TreeKind::Logical, button), [text]);
  assert_eq!(children(&tree, TreeKind::Visual, button), [border]);
}

// Adding a node to a new parent removes it from its old parent's children in the same tree.
#[test]
fn reparent_removes_from_old_parent() {
  let mut tree = UiTree::new();
  let [old_parent, new_parent, child] = nodes(&mut tree, 3)[..] else {
    unreachable!()
  };
  tree.add_child(TreeKind::Logical, old_parent, child);

  tree.add_child(TreeKind::Logical, new_parent, child);

  assert_eq!(children(&tree, TreeKind::Logical, old_parent), []);
  assert_eq!(children(&tree, TreeKind::Logical, new_parent), [child]);
  assert_eq!(tree.parent(TreeKind::Logical, child), Some(new_parent));
}

// `insert_child` places a child at its index, and moving a node within the same parent counts the
// index after it left its old place.
#[test]
fn insert_child_places_the_child_at_its_index() {
  let mut tree = UiTree::new();
  let [parent, a, b, c] = nodes(&mut tree, 4)[..] else {
    unreachable!()
  };
  tree.add_child(TreeKind::Visual, parent, a);
  tree.add_child(TreeKind::Visual, parent, b);

  tree.insert_child(TreeKind::Visual, parent, 1, c);
  assert_eq!(children(&tree, TreeKind::Visual, parent), [a, c, b]);

  tree.insert_child(TreeKind::Visual, parent, 0, b);
  assert_eq!(children(&tree, TreeKind::Visual, parent), [b, a, c]);
}

// `detach` cuts a node from its parent and returns the parent; a node without a parent gives
// `None`.
#[test]
fn detach_returns_the_former_parent() {
  let mut tree = UiTree::new();
  let [parent, child] = nodes(&mut tree, 2)[..] else {
    unreachable!()
  };
  tree.add_child(TreeKind::Logical, parent, child);

  assert_eq!(tree.detach(TreeKind::Logical, child), Some(parent));
  assert_eq!(tree.detach(TreeKind::Logical, child), None);
  assert_eq!(children(&tree, TreeKind::Logical, parent), []);
}

// A stale id has no children.
#[test]
fn stale_node_has_no_children() {
  let mut tree = UiTree::new();
  let [parent, child] = nodes(&mut tree, 2)[..] else {
    unreachable!()
  };
  tree.add_child(TreeKind::Logical, parent, child);

  tree.free(parent);

  assert_eq!(children(&tree, TreeKind::Logical, parent), []);
}

// A stale id doesn't reach the links of the node that reuses its slot: it has no children, while
// the new node has its own.
#[test]
fn stale_id_does_not_reach_the_node_in_its_slot() {
  let mut tree = UiTree::new();
  let [root, parent, child, other] = nodes(&mut tree, 4)[..] else {
    unreachable!()
  };
  tree.add_child(TreeKind::Logical, parent, child);
  tree.free(parent);
  let reused = tree.alloc(Box::new(Control));
  tree.add_child(TreeKind::Logical, root, reused);
  tree.add_child(TreeKind::Logical, reused, other);

  assert_eq!(children(&tree, TreeKind::Logical, parent), []);
  assert_eq!(children(&tree, TreeKind::Logical, reused), [other]);
}

// Adding a stale node as a child panics.
#[test]
#[should_panic(expected = "is not in the arena")]
fn adding_a_stale_child_panics() {
  let mut tree = UiTree::new();
  let [parent, child] = nodes(&mut tree, 2)[..] else {
    unreachable!()
  };
  tree.free(child);

  tree.add_child(TreeKind::Logical, parent, child);
}

// Inserting past the end of the child list panics.
#[test]
#[should_panic(expected = "child index 1 is out of range for 0 children")]
fn insert_past_the_end_panics() {
  let mut tree = UiTree::new();
  let [parent, child] = nodes(&mut tree, 2)[..] else {
    unreachable!()
  };

  tree.insert_child(TreeKind::Logical, parent, 1, child);
}

// A failed edit changes nothing: the child stays under its old parent when the new parent is stale
// or the index is out of range.
#[test]
fn failed_edit_leaves_the_tree_unchanged() {
  let mut tree = UiTree::new();
  let [parent, child, stale] = nodes(&mut tree, 3)[..] else {
    unreachable!()
  };
  tree.add_child(TreeKind::Logical, parent, child);
  tree.free(stale);

  let stale_parent = catch_unwind(AssertUnwindSafe(|| tree.add_child(TreeKind::Logical, stale, child)));
  let past_the_end = catch_unwind(AssertUnwindSafe(|| tree.insert_child(TreeKind::Logical, parent, 2, child)));

  assert!(stale_parent.is_err() && past_the_end.is_err());
  assert_eq!(tree.parent(TreeKind::Logical, child), Some(parent));
  assert_eq!(children(&tree, TreeKind::Logical, parent), [child]);
}

// `ancestors` walks from the parent up to the root, in one tree only.
#[test]
fn ancestors_walk_up_to_the_root() {
  let mut tree = UiTree::new();
  let [root, middle, leaf] = nodes(&mut tree, 3)[..] else {
    unreachable!()
  };
  tree.add_child(TreeKind::Logical, root, middle);
  tree.add_child(TreeKind::Logical, middle, leaf);

  assert_eq!(tree.ancestors(TreeKind::Logical, leaf).collect::<Vec<_>>(), [middle, root]);
  assert_eq!(tree.ancestors(TreeKind::Visual, leaf).count(), 0);
}

// Adding a node under itself or under one of its descendants is rejected, and the tree is left as
// it was.
#[test]
fn cycle_insert_is_rejected() {
  let mut tree = UiTree::new();
  let [root, middle, leaf] = nodes(&mut tree, 3)[..] else {
    unreachable!()
  };
  tree.add_child(TreeKind::Logical, root, middle);
  tree.add_child(TreeKind::Logical, middle, leaf);

  let under_itself = catch_unwind(AssertUnwindSafe(|| tree.add_child(TreeKind::Logical, middle, middle)));
  let under_descendant = catch_unwind(AssertUnwindSafe(|| tree.insert_child(TreeKind::Logical, leaf, 0, root)));

  assert!(under_itself.is_err() && under_descendant.is_err());
  assert_eq!(tree.parent(TreeKind::Logical, middle), Some(root));
  assert_eq!(tree.parent(TreeKind::Logical, root), None);
  assert_eq!(children(&tree, TreeKind::Logical, leaf), []);
}

// The cycle check is per tree: a logical ancestor can be a visual child.
#[test]
fn cycle_check_looks_at_one_tree_only() {
  let mut tree = UiTree::new();
  let [root, leaf] = nodes(&mut tree, 2)[..] else {
    unreachable!()
  };
  tree.add_child(TreeKind::Logical, root, leaf);

  tree.add_child(TreeKind::Visual, leaf, root);

  assert_eq!(tree.parent(TreeKind::Visual, root), Some(leaf));
}

// The cycle panic names both nodes and the tree.
#[test]
#[should_panic(expected = "would create a cycle in the Visual tree")]
fn cycle_panic_names_the_tree() {
  let mut tree = UiTree::new();
  let [node] = nodes(&mut tree, 1)[..] else { unreachable!() };

  tree.add_child(TreeKind::Visual, node, node);
}

// `descendants` yields a subtree in pre-order, and each tree follows its own links.
#[test]
fn descendants_preorder() {
  let mut tree = UiTree::new();
  let [a, b, c, d, e] = nodes(&mut tree, 5)[..] else {
    unreachable!()
  };
  // Logical: a(b(c, d), e)
  tree.add_child(TreeKind::Logical, a, b);
  tree.add_child(TreeKind::Logical, b, c);
  tree.add_child(TreeKind::Logical, b, d);
  tree.add_child(TreeKind::Logical, a, e);
  // Visual: a(e(d, b(c)))
  tree.add_child(TreeKind::Visual, a, e);
  tree.add_child(TreeKind::Visual, e, d);
  tree.add_child(TreeKind::Visual, e, b);
  tree.add_child(TreeKind::Visual, b, c);

  assert_eq!(tree.descendants(TreeKind::Logical, a).collect::<Vec<_>>(), [b, c, d, e]);
  assert_eq!(tree.descendants(TreeKind::Visual, a).collect::<Vec<_>>(), [e, d, b, c]);
  assert_eq!(tree.descendants(TreeKind::Logical, c).count(), 0);
}

// Freeing a control frees its logical content and the visual-only parts of its template, and
// nothing else.
#[test]
fn free_frees_all_descendants() {
  let mut tree = UiTree::new();
  let [panel, button, border, presenter, text, sibling] = nodes(&mut tree, 6)[..] else {
    unreachable!()
  };
  tree.add_child(TreeKind::Logical, panel, button);
  tree.add_child(TreeKind::Logical, panel, sibling);
  tree.add_child(TreeKind::Logical, button, text);
  tree.add_child(TreeKind::Visual, panel, button);
  tree.add_child(TreeKind::Visual, button, border);
  tree.add_child(TreeKind::Visual, border, presenter);
  tree.add_child(TreeKind::Visual, presenter, text);

  tree.free(button);

  assert_eq!(tree.len(), 2);
  assert!(tree.contains(panel) && tree.contains(sibling));
  assert_eq!(children(&tree, TreeKind::Logical, panel), [sibling]);
  assert_eq!(children(&tree, TreeKind::Visual, panel), []);
}

// A node inside the subtree in one tree but under an outside parent in the other is freed, and the
// outside parent stops listing it.
#[test]
fn free_detaches_nodes_from_outside_parents() {
  let mut tree = UiTree::new();
  let [owner, popup_root, content] = nodes(&mut tree, 3)[..] else {
    unreachable!()
  };
  tree.add_child(TreeKind::Logical, owner, content);
  tree.add_child(TreeKind::Visual, popup_root, content);

  tree.free(owner);

  assert!(!tree.contains(content));
  assert_eq!(children(&tree, TreeKind::Visual, popup_root), []);
  assert_eq!(tree.len(), 1);
}

// Freeing a root frees the whole tree.
#[test]
fn free_of_a_root_empties_the_arena() {
  let mut tree = UiTree::new();
  let [root, a, b] = nodes(&mut tree, 3)[..] else {
    unreachable!()
  };
  tree.add_child(TreeKind::Logical, root, a);
  tree.add_child(TreeKind::Visual, a, b);

  tree.free(root);

  assert!(tree.is_empty());
}

// Adding, moving or removing a visual child marks the parent for layout; logical edits don't.
#[test]
fn visual_edits_mark_the_parent_for_layout() {
  let mut tree = UiTree::new();
  let ids = nodes(&mut tree, 3);
  let [panel, other, child] = ids[..] else { unreachable!() };
  clean(&mut tree, &ids);

  tree.add_child(TreeKind::Logical, panel, child);
  assert_eq!(tree.dirty(panel), Affects::NONE, "logical edits leave layout alone");

  tree.add_child(TreeKind::Visual, panel, child);
  assert_eq!(tree.dirty(panel), Affects::LAYOUT);

  clean(&mut tree, &ids);
  tree.add_child(TreeKind::Visual, other, child);
  assert_eq!(
    (tree.dirty(panel), tree.dirty(other)),
    (Affects::LAYOUT, Affects::LAYOUT),
    "both parents of a move"
  );

  clean(&mut tree, &ids);
  tree.detach(TreeKind::Visual, child);
  assert_eq!(tree.dirty(other), Affects::LAYOUT);
}

// `check_link` reports, without changing anything, each case `add_child` and `insert_child` panic on.
#[test]
fn check_link_reports_without_changing_anything() {
  let mut tree = UiTree::new();
  let [parent, child, stale] = nodes(&mut tree, 3)[..] else {
    unreachable!()
  };
  tree.free(stale);
  tree.add_child(TreeKind::Visual, parent, child);

  assert_eq!(tree.check_link(TreeKind::Visual, parent, None, child), Ok(()));
  assert_eq!(
    tree.check_link(TreeKind::Visual, parent, None, stale),
    Err(LinkError::NotInTree(stale))
  );
  assert_eq!(
    tree.check_link(TreeKind::Visual, child, None, parent),
    Err(LinkError::Cycle {
      tree: TreeKind::Visual,
      parent: child,
      child: parent
    })
  );
  assert_eq!(
    tree.check_link(TreeKind::Logical, parent, Some(1), child),
    Err(LinkError::IndexOutOfRange { index: 1, len: 0 })
  );
  assert_eq!(tree.check_link(TreeKind::Visual, parent, Some(0), child), Ok(()));
  assert_eq!(children(&tree, TreeKind::Visual, parent), [child]);
}

// Freeing a node frees its children and cuts it from its parent, so no live node names it.
#[test]
fn free_detaches_and_frees_the_subtree() {
  let mut tree = UiTree::new();
  let [root, parent, child] = nodes(&mut tree, 3)[..] else {
    unreachable!()
  };
  tree.add_child(TreeKind::Logical, root, parent);
  tree.add_child(TreeKind::Logical, parent, child);

  tree.free(parent);

  assert!(!tree.contains(child));
  assert_eq!(children(&tree, TreeKind::Logical, root), []);
}

// Freeing a stale id is a bug, as using one anywhere else in the tree is.
#[test]
#[should_panic(expected = "is not in the arena")]
fn free_of_a_stale_id_panics() {
  let mut tree = UiTree::new();
  let [node] = nodes(&mut tree, 1)[..] else { unreachable!() };
  tree.free(node);

  tree.free(node);
}

// `TreeKind::ALL` names both trees, logical first.
#[test]
fn tree_kind_all_lists_logical_then_visual() {
  assert_eq!(TreeKind::ALL, [TreeKind::Logical, TreeKind::Visual]);
}
