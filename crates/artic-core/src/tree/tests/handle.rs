use crate::Object;
use crate::property::AnyProperty;
use crate::tree::{Handle, NodeId, UiTree};
use pretty_assertions::assert_eq;
use static_assertions::{assert_eq_size, assert_impl_all};

#[derive(Debug, PartialEq, Eq)]
struct Label(&'static str);

impl Object for Label {
  fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
}

assert_eq_size!(Handle<Label>, NodeId);
assert_impl_all!(Handle<Label>: Copy, Eq, std::hash::Hash, std::fmt::Debug, Send, Sync);

// A handle from `alloc_typed` reads and writes its node as its own type.
#[test]
fn handle_reads_and_writes_its_node() {
  let mut tree = UiTree::new();

  let label = tree.alloc_typed(Label("hi"));
  label.get_mut(&mut tree).0 = "bye";

  assert_eq!(label.get(&tree), &Label("bye"));
  assert_eq!(tree.get_as::<Label>(label.id()), Some(&Label("bye")));
}

// A handle converts to the id of the node it names.
#[test]
fn handle_converts_to_its_id() {
  let mut tree = UiTree::new();

  let label = tree.alloc_typed(Label("hi"));
  let id: NodeId = label.into();

  assert_eq!(id, label.id());
}

/// The slot part of an id's debug text, which a reused slot shares with the id it replaced.
fn slot(id: NodeId) -> String {
  let text = format!("{id:?}");
  text.split("generation").next().expect("split yields one part").to_owned()
}

// After its node is freed, a handle finds nothing, even once its slot holds another node of the
// same type.
#[test]
fn handle_to_a_freed_node_is_none() {
  let mut tree = UiTree::new();
  let label = tree.alloc_typed(Label("old"));

  tree.free(label.id());
  let reused = tree.alloc_typed(Label("new"));

  assert_eq!(slot(reused.id()), slot(label.id()));
  assert_eq!(label.try_get(&tree), None);
}

// Reading through a handle whose node was freed is a bug, and panics.
#[test]
#[should_panic(expected = "freed")]
fn get_on_a_freed_node_panics() {
  let mut tree = UiTree::new();
  let label = tree.alloc_typed(Label("old"));
  tree.free(label.id());

  let _ = label.get(&tree);
}

// Handles compare by the node they name.
#[test]
fn handles_compare_by_node() {
  let mut tree = UiTree::new();

  let first = tree.alloc_typed(Label("a"));
  let second = tree.alloc_typed(Label("a"));
  let copy = first;

  assert_eq!(first, copy);
  assert!(first != second);
}

// Writing through a handle whose node was freed is a bug, and panics.
#[test]
#[should_panic(expected = "freed")]
fn get_mut_on_a_freed_node_panics() {
  let mut tree = UiTree::new();
  let label = tree.alloc_typed(Label("old"));
  tree.free(label.id());

  let _ = label.get_mut(&mut tree);
}
