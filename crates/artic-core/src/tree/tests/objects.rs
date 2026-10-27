use crate::Object;
use crate::property::PropertyVisitor;
use crate::tree::{NodeId, TreeKind, UiTree};
use pretty_assertions::{assert_eq, assert_ne};
use static_assertions::{assert_eq_size, assert_impl_all, assert_not_impl_any};

assert_eq_size!(NodeId, Option<NodeId>);
assert_eq_size!(NodeId, u64);
assert_not_impl_any!(UiTree: Send);
assert_impl_all!(NodeId: Copy, Send, Sync);

#[derive(Debug, PartialEq, Eq)]
struct Control;

#[derive(Debug, PartialEq, Eq)]
struct OtherControl;

impl Object for Control {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

impl Object for OtherControl {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

#[test]
fn get_as_wrong_type_returns_none() {
  let mut tree = UiTree::new();
  let control = Box::new(Control);
  let n1 = tree.alloc(control);
  assert_eq!(tree.get_as::<Control>(n1), Some(&Control));
  assert!(tree.get_as::<OtherControl>(n1).is_none());
}
#[test]
fn get_as_mut_wrong_type_returns_none() {
  let mut tree = UiTree::new();
  let n1 = tree.alloc(Box::new(Control));
  assert!(tree.get_as_mut::<OtherControl>(n1).is_none());
}

#[derive(Debug, PartialEq, Eq)]
struct Counter(u32);

impl Object for Counter {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

#[test]
fn nodes_of_different_types_share_one_arena() {
  let mut tree = UiTree::new();
  let control = tree.alloc(Box::new(Control));
  let counter = tree.alloc(Box::new(Counter(7)));
  assert_eq!(tree.len(), 2);
  assert_eq!(tree.get_as::<Control>(control), Some(&Control));
  assert_eq!(tree.get_as::<Counter>(counter), Some(&Counter(7)));
  assert!(tree.get_as::<Counter>(control).is_none());
  assert!(tree.get_as::<Control>(counter).is_none());
}

#[test]
fn get_as_mut_changes_are_visible_through_get_as() {
  let mut tree = UiTree::new();
  let n1 = tree.alloc(Box::new(Counter(1)));
  tree.get_as_mut::<Counter>(n1).unwrap().0 += 1;
  assert_eq!(tree.get_as::<Counter>(n1), Some(&Counter(2)));
}

#[test]
fn get_and_get_mut_return_the_node_as_object() {
  let mut tree = UiTree::new();
  let n1 = tree.alloc(Box::new(Counter(3)));
  assert_eq!(format!("{:?}", tree.get(n1).unwrap()), "Counter(3)");
  assert_eq!(format!("{:?}", tree.get_mut(n1).unwrap()), "Counter(3)");
}

#[test]
fn stale_id_returns_none_from_every_accessor() {
  let mut tree = UiTree::new();
  let n1 = tree.alloc(Box::new(Control));
  tree.free(n1);
  let n2 = tree.alloc(Box::new(Control));
  assert!(tree.get(n1).is_none());
  assert!(tree.get_mut(n1).is_none());
  assert!(tree.get_as::<Control>(n1).is_none());
  assert!(tree.get_as_mut::<Control>(n1).is_none());
  assert!(!tree.contains(n1));
  assert!(tree.contains(n2));
  assert_eq!(tree.len(), 1);
}

#[test]
fn copied_node_ids_are_equal_and_distinct_nodes_are_not() {
  let mut tree = UiTree::new();
  let n1 = tree.alloc(Box::new(Control));
  let n2 = tree.alloc(Box::new(Control));
  let copy = n1;
  assert_eq!(copy, n1);
  assert_ne!(n1, n2);
}

#[test]
fn default_tree_arena_is_empty() {
  let tree = UiTree::default();
  assert!(tree.is_empty());
  assert_eq!(tree.len(), 0);
}

// A new node has no parent in either tree, and a stale id has none either.
#[test]
fn new_node_has_no_parent() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Control));

  assert_eq!(tree.parent(TreeKind::Logical, node), None);
  assert_eq!(tree.parent(TreeKind::Visual, node), None);
  tree.free(node);
  assert_eq!(tree.parent(TreeKind::Logical, node), None);
}
