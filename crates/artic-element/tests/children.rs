// Children and Child keep a node's logical and visual children the same, and refuse a change that
// would leave the two trees apart.

use artic_core::Object;
use artic_core::property::PropertyVisitor;
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_element::{Child, Children};
use artic_registry::{UiCollection, ValueError};
use pretty_assertions::assert_eq;

#[derive(Debug, Default)]
struct Node {
  children: Children,
  child: Child,
}

impl Object for Node {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

fn node(tree: &mut UiTree) -> NodeId {
  tree.alloc(Box::new(Node::default()))
}

fn both(tree: &UiTree, parent: NodeId) -> (Vec<NodeId>, Vec<NodeId>) {
  (
    tree.children(TreeKind::Logical, parent).collect(),
    tree.children(TreeKind::Visual, parent).collect(),
  )
}

fn children_field(tree: &mut UiTree, owner: NodeId) -> Option<&mut Children> {
  tree.get_as_mut::<Node>(owner).map(|node| &mut node.children)
}

fn child_field(tree: &mut UiTree, owner: NodeId) -> Option<&mut Child> {
  tree.get_as_mut::<Node>(owner).map(|node| &mut node.child)
}

// Adding, inserting and removing change the logical and the visual children alike.
#[test]
fn panel_children_update_tree_links() {
  let mut tree = UiTree::new();
  let (panel, a, b, c) = (node(&mut tree), node(&mut tree), node(&mut tree), node(&mut tree));

  Children::add(&mut tree, panel, a);
  Children::add(&mut tree, panel, c);
  Children::insert(&mut tree, panel, 1, b);
  assert_eq!(both(&tree, panel), (vec![a, b, c], vec![a, b, c]));

  Children::remove(&mut tree, panel, b);
  assert_eq!(both(&tree, panel), (vec![a, c], vec![a, c]));
  assert!(tree.contains(b));
}

// A child under another parent in one tree only is moved in both.
#[test]
fn add_moves_a_child_in_both_trees() {
  let mut tree = UiTree::new();
  let (old, panel, child) = (node(&mut tree), node(&mut tree), node(&mut tree));
  tree.add_child(TreeKind::Visual, old, child);

  Children::add(&mut tree, panel, child);

  assert_eq!(both(&tree, panel), (vec![child], vec![child]));
  assert_eq!(tree.parent(TreeKind::Visual, child), Some(panel));
}

// An insert that one tree can't take changes neither tree: index 1 is valid among the visual
// children but past the logical ones.
#[test]
fn add_checks_both_trees_before_linking() {
  let mut tree = UiTree::new();
  let (panel, a, b) = (node(&mut tree), node(&mut tree), node(&mut tree));
  tree.add_child(TreeKind::Visual, panel, a);

  let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Children::insert(&mut tree, panel, 1, b)));

  assert!(refused.is_err());
  assert_eq!(both(&tree, panel), (vec![], vec![a]));
  assert_eq!(tree.parent(TreeKind::Visual, b), None);
}

// Removing a node that isn't this parent's child leaves the other parent's links alone.
#[test]
#[should_panic(expected = "is not a child of")]
fn remove_leaves_another_parents_child_alone() {
  let mut tree = UiTree::new();
  let (panel, other, child) = (node(&mut tree), node(&mut tree), node(&mut tree));
  Children::add(&mut tree, other, child);

  Children::remove(&mut tree, panel, child);
}

// A view that names a node not in the tree is refused with the link problem, not a panic.
#[test]
fn push_child_refuses_nodes_not_in_the_tree() {
  let mut tree = UiTree::new();
  let (panel, gone) = (node(&mut tree), node(&mut tree));
  tree.free(gone);

  let result = Children::push_child(&mut tree, panel, gone, children_field);

  assert!(matches!(result, Err(ValueError::InvalidChild(_))));
  assert_eq!(both(&tree, panel), (vec![], vec![]));
}

// Setting a single child replaces the old one in both trees.
#[test]
fn child_set_replaces_in_both_trees() {
  let mut tree = UiTree::new();
  let (border, first, second) = (node(&mut tree), node(&mut tree), node(&mut tree));

  Child::set(&mut tree, border, Some(first));
  Child::set(&mut tree, border, Some(second));

  assert_eq!(Child::get(&tree, border), Some(second));
  assert_eq!(both(&tree, border), (vec![second], vec![second]));
  assert_eq!(tree.parent(TreeKind::Logical, first), None);
}

// A current child linked elsewhere in one tree is a bug: nothing changes.
#[test]
#[should_panic(expected = "children of")]
fn child_set_leaves_a_mislinked_child_alone() {
  let mut tree = UiTree::new();
  let (border, other, first, second) = (node(&mut tree), node(&mut tree), node(&mut tree), node(&mut tree));
  Child::set(&mut tree, border, Some(first));
  tree.add_child(TreeKind::Visual, other, first);

  Child::set(&mut tree, border, Some(second));
}

// A view that gives a single-child element a second child is refused.
#[test]
fn second_child_is_refused() {
  let mut tree = UiTree::new();
  let (border, first, second) = (node(&mut tree), node(&mut tree), node(&mut tree));
  Child::push_child(&mut tree, border, first, child_field).expect("first");

  assert_eq!(
    Child::push_child(&mut tree, border, second, child_field),
    Err(ValueError::OneChildOnly)
  );
  assert_eq!(Child::get(&tree, border), Some(first));
}

// When the two trees don't hold the same single child, `set` refuses and changes nothing.
#[test]
fn child_set_refuses_trees_that_disagree() {
  let mut tree = UiTree::new();
  let (border, old, extra, new) = (node(&mut tree), node(&mut tree), node(&mut tree), node(&mut tree));
  Child::set(&mut tree, border, Some(old));
  tree.insert_child(TreeKind::Visual, border, 0, extra);

  let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Child::set(&mut tree, border, Some(new))));

  assert!(refused.is_err());
  assert_eq!(both(&tree, border), (vec![old], vec![extra, old]));
  assert_eq!(tree.parent(TreeKind::Logical, new), None);
}

// When a panel's two trees already hold different children, adding, inserting and removing refuse
// and change nothing.
#[test]
fn children_refuse_trees_that_disagree() {
  let mut tree = UiTree::new();
  let (panel, old, extra, new) = (node(&mut tree), node(&mut tree), node(&mut tree), node(&mut tree));
  Children::add(&mut tree, panel, old);
  tree.insert_child(TreeKind::Visual, panel, 0, extra);

  let add = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Children::add(&mut tree, panel, new)));
  let insert = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Children::insert(&mut tree, panel, 0, new)));
  let remove = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Children::remove(&mut tree, panel, old)));
  let pushed = Children::push_child(&mut tree, panel, new, children_field);

  assert!(add.is_err() && insert.is_err() && remove.is_err());
  assert!(matches!(pushed, Err(ValueError::InvalidChild(_))));
  assert_eq!(both(&tree, panel), (vec![old], vec![extra, old]));
}
