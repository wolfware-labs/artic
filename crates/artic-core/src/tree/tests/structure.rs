use super::nodes;
use crate::property::{Affects, AttachedKey, Inheritance};
use crate::tree::{TreeKind, UiTree};
use pretty_assertions::{assert_eq, assert_ne};

static DEPTH: AttachedKey<i32> = AttachedKey::new("Depth", Affects::LAYOUT, Inheritance::Replace, || 0);

// Linking in one tree or both changes the structure generation.
#[test]
fn links_change_the_structure_generation() {
  let mut tree = UiTree::new();
  let [panel, a, b, c] = nodes(&mut tree);

  let before = tree.structure_generation();
  tree.add_child(TreeKind::Logical, panel, a);
  assert_ne!(tree.structure_generation(), before);

  let before = tree.structure_generation();
  tree.insert_child(TreeKind::Visual, panel, 0, b);
  assert_ne!(tree.structure_generation(), before);

  let before = tree.structure_generation();
  tree.link_both(c, None, b).expect("a link both trees take");
  assert_ne!(tree.structure_generation(), before);
}

// Cutting a link changes the structure generation.
#[test]
fn cuts_change_the_structure_generation() {
  let mut tree = UiTree::new();
  let [panel, a, b] = nodes(&mut tree);
  tree.link_both(panel, None, b).expect("a link both trees take");
  tree.add_child(TreeKind::Visual, a, panel);

  let before = tree.structure_generation();
  tree.unlink_both(panel, b).expect("b is the panel's child");
  assert_ne!(tree.structure_generation(), before);

  let before = tree.structure_generation();
  tree.detach(TreeKind::Visual, panel);
  assert_ne!(tree.structure_generation(), before);
}

// Allocating a node and freeing one, alone or with its subtree, change the structure generation.
#[test]
fn alloc_and_free_change_the_structure_generation() {
  let mut tree = UiTree::new();
  let [panel, child, grandchild] = nodes(&mut tree);
  tree.add_child(TreeKind::Visual, panel, child);
  tree.add_child(TreeKind::Visual, child, grandchild);

  let before = tree.structure_generation();
  let [loose] = nodes(&mut tree);
  assert_ne!(tree.structure_generation(), before);

  let before = tree.structure_generation();
  tree.free(loose);
  assert_ne!(tree.structure_generation(), before);

  let before = tree.structure_generation();
  tree.free(child);
  assert_ne!(tree.structure_generation(), before);
  assert!(!tree.contains(grandchild));
}

// Linking a child where it already is counts: the link is cut and made again.
#[test]
fn relinking_in_place_changes_the_structure_generation() {
  let mut tree = UiTree::new();
  let [panel, a] = nodes(&mut tree);
  tree.add_child(TreeKind::Logical, panel, a);

  let before = tree.structure_generation();
  tree.add_child(TreeKind::Logical, panel, a);
  assert_ne!(tree.structure_generation(), before);
}

// Edits that make or cut no link leave the structure generation as it was, an inherited attached
// value included.
#[test]
fn other_edits_leave_the_structure_generation() {
  let mut tree = UiTree::new();
  let [panel, a, b, loose] = nodes(&mut tree);
  tree.add_child(TreeKind::Visual, panel, a);
  tree.add_child(TreeKind::Logical, panel, a);
  let before = tree.structure_generation();

  tree.attached_property(panel, &DEPTH).set(2);
  tree.attached_property(a, &DEPTH);
  tree.mark_dirty(a, Affects::LAYOUT);
  tree.clear_dirty(a, Affects::LAYOUT);
  let _ = tree.get_mut(a);
  tree.detach(TreeKind::Logical, loose);
  tree.link_both(a, None, panel).expect_err("a cycle");
  tree.unlink_both(loose, b).expect_err("b isn't loose's child");

  assert_eq!(tree.structure_generation(), before);
}
