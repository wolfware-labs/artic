use crate::Object;
use crate::property::{Affects, AttachedKey, Inheritance, Property, PropertyKey, PropertyVisitor};
use crate::reactive::{Scope, live_counts};
use crate::tree::{TreeKind, UiTree};
use pretty_assertions::assert_eq;

static FONT_SIZE: PropertyKey<i32> = PropertyKey::new("FontSize", Affects::LAYOUT, Inheritance::Replace);
static FOREGROUND: PropertyKey<i32> = PropertyKey::new("Foreground", Affects::RENDER, Inheritance::None);
static ROW: AttachedKey<usize> = AttachedKey::new("Row", Affects::LAYOUT, Inheritance::None, || 0);

#[derive(Debug)]
struct Label {
  font_size: Property<i32>,
  foreground: Property<i32>,
}

impl Label {
  fn new() -> Label {
    Label {
      font_size: Property::new(&FONT_SIZE, 12),
      foreground: Property::new(&FOREGROUND, 0),
    }
  }
}

impl Object for Label {
  fn visit_properties(&self, visitor: &mut PropertyVisitor<'_>) {
    visitor.visit(&self.font_size);
    visitor.visit(&self.foreground);
  }
}

// A freed node's properties go with it: the reactive runtime is back where it was.
#[test]
fn free_disposes_the_properties() {
  let baseline = live_counts();
  let mut tree = UiTree::new();
  let label = tree.alloc(Box::new(Label::new()));

  tree.free(label);

  assert_eq!(live_counts(), baseline);
}

// Attached values are the node's properties too.
#[test]
fn free_disposes_attached_values() {
  let baseline = live_counts();
  let mut tree = UiTree::new();
  let label = tree.alloc(Box::new(Label::new()));
  tree.attached_property(label, &ROW).set(2);

  tree.free(label);

  assert_eq!(live_counts(), baseline);
}

// A property created in a scope can still be disposed with the scope after the tree freed it.
#[test]
fn disposing_the_scope_after_free_is_fine() {
  let baseline = live_counts();
  let view = Scope::new_root();
  let mut tree = UiTree::new();
  let label = tree.alloc(Box::new(view.run(Label::new)));

  tree.free(label);
  view.dispose();

  assert_eq!(live_counts(), baseline);
}

// A parent keeps working after its child is freed: a write marks the parent, and the child's
// sibling still follows it.
#[test]
fn free_keeps_the_parent_working() {
  let mut tree = UiTree::new();
  let [parent, child, sibling] = [(); 3].map(|_| tree.alloc(Box::new(Label::new())));
  tree.add_child(TreeKind::Logical, parent, child);
  tree.add_child(TreeKind::Logical, parent, sibling);
  tree.free(child);
  tree.clear_dirty(parent, Affects::LAYOUT | Affects::RENDER);
  tree.clear_dirty(sibling, Affects::LAYOUT | Affects::RENDER);

  tree.get_as::<Label>(parent).unwrap().font_size.set(20);

  assert_eq!(tree.dirty(parent), Affects::LAYOUT);
  assert_eq!(tree.get_as::<Label>(sibling).unwrap().font_size.get(), 20);
  assert_eq!(tree.dirty(sibling), Affects::LAYOUT);
}

/// A node that lists the same property twice.
#[derive(Debug)]
struct Twice(Property<i32>);

impl Object for Twice {
  fn visit_properties(&self, visitor: &mut PropertyVisitor<'_>) {
    visitor.visit(&self.0);
    visitor.visit(&self.0);
  }
}

// A property a node lists twice is disposed once, and freeing goes on.
#[test]
fn free_disposes_a_property_listed_twice_once() {
  let baseline = live_counts();
  let mut tree = UiTree::new();
  let twice = tree.alloc(Box::new(Twice(Property::new(&FOREGROUND, 0))));

  tree.free(twice);

  assert_eq!(live_counts(), baseline);
}

// A property whose scope was disposed first is skipped: the tree frees the node anyway.
#[test]
fn free_skips_properties_their_scope_disposed() {
  let baseline = live_counts();
  let view = Scope::new_root();
  let mut tree = UiTree::new();
  let [parent, child] = [(); 2].map(|_| tree.alloc(Box::new(view.run(Label::new))));
  tree.add_child(TreeKind::Logical, parent, child);
  view.dispose();

  tree.free(parent);

  assert!(tree.is_empty());
  assert_eq!(live_counts(), baseline);
}

// An attached value belongs to its node, not to the scope that happened to be active when it was
// first set: disposing that scope leaves it, and freeing the node frees it.
#[test]
fn attached_values_belong_to_the_node() {
  let baseline = live_counts();
  let view = Scope::new_root();
  let mut tree = UiTree::new();
  let label = tree.alloc(Box::new(Label::new()));
  view.run(|| tree.attached_property(label, &ROW).set(3));
  view.dispose();

  assert_eq!(tree.attached(label, &ROW), 3);
  tree.free(label);
  assert_eq!(live_counts(), baseline);
}

// Dropping a tree lets go of every node it holds, as freeing them would.
#[test]
fn dropping_the_tree_disposes_its_nodes_properties() {
  let baseline = live_counts();
  let mut tree = UiTree::new();
  let label = tree.alloc(Box::new(Label::new()));
  tree.attached_property(label, &ROW).set(2);

  drop(tree);

  assert_eq!(live_counts(), baseline);
}
