use crate::Object;
use crate::property::{Affects, AnyProperty, AttachedKey, Inheritance, Precedence, Property, inherit};
use crate::tree::{NodeId, TreeKind, UiTree};
use pretty_assertions::assert_eq;

static ROW: AttachedKey<usize> = AttachedKey::new("Row", Affects::LAYOUT, Inheritance::None, || 0);
static LABEL: AttachedKey<String> = AttachedKey::new("Label", Affects::RENDER, Inheritance::None, || "none".to_owned());

#[derive(Debug)]
struct Button;

impl Object for Button {
  fn visit_properties(&self, _visit: &mut dyn FnMut(AnyProperty<'_>)) {}
}

/// A tree with one button whose dirty flags are already cleared.
fn clean_button() -> (UiTree, NodeId) {
  let mut tree = UiTree::new();
  let button = tree.alloc(Box::new(Button));
  tree.clear_dirty(button, Affects::LAYOUT | Affects::RENDER);
  (tree, button)
}

// Reading an attached property nothing set returns its default and stores nothing.
#[test]
fn unset_value_is_the_default_without_storing() {
  let (tree, button) = clean_button();

  assert_eq!(tree.attached(button, &ROW), 0);
  assert_eq!(tree.attached(button, &LABEL), "none");
  assert_eq!(tree.attached_values.count(button), 0);
}

// A set value reads back, and the property is created once.
#[test]
fn set_value_reads_back() {
  let (mut tree, button) = clean_button();

  tree.attached_property(button, &ROW).set(3);

  assert_eq!(tree.attached(button, &ROW), 3);
  assert_eq!(tree.attached_property(button, &ROW).get(), 3);
  assert_eq!(tree.attached_values.count(button), 1);
}

// A local value overrides a style value, which shows again when the local value is cleared.
#[test]
fn local_overrides_style() {
  let (mut tree, button) = clean_button();
  let row = tree.attached_property(button, &ROW);

  row.set_at(Precedence::StyleSetter, 2);
  row.set(1);
  assert_eq!(tree.attached(button, &ROW), 1);

  row.clear(Precedence::Local);
  assert_eq!(tree.attached(button, &ROW), 2);
}

// Two attached properties on one node are independent.
#[test]
fn kinds_are_independent() {
  let (mut tree, button) = clean_button();

  tree.attached_property(button, &ROW).set(4);
  tree.attached_property(button, &LABEL).set("ok".to_owned());

  assert_eq!(tree.attached(button, &ROW), 4);
  assert_eq!(tree.attached(button, &LABEL), "ok");
}

// A change marks the node dirty with the key's stages, like a field.
#[test]
fn change_marks_the_node_dirty() {
  let (mut tree, button) = clean_button();

  tree.attached_property(button, &LABEL).set("ok".to_owned());

  assert_eq!(tree.dirty(button), Affects::RENDER);
}

// The values go with their node: a node that later takes the slot starts at the defaults.
#[test]
fn values_go_with_their_node() {
  let (mut tree, button) = clean_button();
  tree.attached_property(button, &ROW).set(5);

  tree.free(button);
  let next = tree.alloc(Box::new(Button));

  assert_eq!(tree.attached(next, &ROW), 0);
}

static CHROME: AttachedKey<u8> = AttachedKey::new("Chrome", Affects::SURFACE, Inheritance::Replace, || 0);

/// Three buttons, each the logical child of the one before.
fn chain(tree: &mut UiTree) -> [NodeId; 3] {
  let nodes = [(); 3].map(|()| tree.alloc(Box::new(Button)));
  tree.add_child(TreeKind::Logical, nodes[0], nodes[1]);
  tree.add_child(TreeKind::Logical, nodes[1], nodes[2]);
  nodes
}

// A node that holds an inheriting attached property follows the nearest logical ancestor that
// holds it, and its own local value replaces what it inherits.
#[test]
fn attached_property_inherits() {
  let mut tree = UiTree::new();
  let [root, _, leaf] = chain(&mut tree);
  tree.attached_property(root, &CHROME).set(2);

  let held = tree.attached_property(leaf, &CHROME);
  assert_eq!(held.get(), 2);
  tree.attached_property(root, &CHROME).set(3);
  assert_eq!(held.get(), 3);
  held.set(1);
  assert_eq!(tree.attached(leaf, &CHROME), 1);
}

// Reading an inheriting attached property a node doesn't hold finds the nearest ancestor's value
// without storing anything; a key that doesn't inherit reads its default.
#[test]
fn attached_read_finds_the_nearest_ancestor_value() {
  let mut tree = UiTree::new();
  let [root, _, leaf] = chain(&mut tree);
  tree.attached_property(root, &CHROME).set(2);
  tree.attached_property(root, &ROW).set(5);

  assert_eq!(tree.attached(leaf, &CHROME), 2);
  assert_eq!(tree.attached(leaf, &ROW), 0);
  assert_eq!(tree.attached_values.count(leaf), 0);
}

// A node already holding the property follows an ancestor that starts holding it later.
#[test]
fn held_property_follows_an_ancestor_set_later() {
  let mut tree = UiTree::new();
  let [root, _, leaf] = chain(&mut tree);
  let held = tree.attached_property(leaf, &CHROME);

  tree.attached_property(root, &CHROME).set(4);

  assert_eq!(held.get(), 4);
}

// An inherited change marks the holder with the key's stages, as a field's would.
#[test]
fn inherited_attached_change_marks_the_holder() {
  let mut tree = UiTree::new();
  let [root, _, leaf] = chain(&mut tree);
  let source = tree.attached_property(root, &CHROME);
  let _held = tree.attached_property(leaf, &CHROME);
  tree.clear_dirty(leaf, Affects::LAYOUT | Affects::RENDER | Affects::SURFACE);

  source.set(5);

  assert_eq!(tree.dirty(leaf), Affects::SURFACE);
}

// A holder moved under another ancestor follows the new one.
#[test]
fn moved_holder_follows_its_new_ancestor() {
  let mut tree = UiTree::new();
  let [first, _, leaf] = chain(&mut tree);
  let second = tree.alloc(Box::new(Button));
  tree.attached_property(first, &CHROME).set(1);
  tree.attached_property(second, &CHROME).set(2);
  let held = tree.attached_property(leaf, &CHROME);

  tree.detach(TreeKind::Logical, leaf);
  tree.add_child(TreeKind::Logical, second, leaf);

  assert_eq!(held.get(), 2);
}

/// A node that holds `Chrome` as a field, keyed by the attached property's key.
#[derive(Debug)]
struct Themed {
  chrome: Property<u8>,
}

impl Object for Themed {
  fn visit_properties(&self, visit: &mut dyn FnMut(AnyProperty<'_>)) {
    visit(self.chrome.as_ref());
  }
}

// A field sharing an inheriting attached key is held like an attached value: reading it on its
// node finds it, and reading it on a descendant agrees with holding it there.
#[test]
fn attached_read_finds_an_ancestor_field() {
  let mut tree = UiTree::new();
  let root = tree.alloc(Box::new(Themed {
    chrome: Property::new(CHROME.key(), 6),
  }));
  let leaf = tree.alloc(Box::new(Button));
  tree.add_child(TreeKind::Logical, root, leaf);

  assert_eq!(tree.attached(root, &CHROME), 6);
  assert_eq!(tree.attached(leaf, &CHROME), 6);
  assert_eq!(tree.attached_property(leaf, &CHROME).get(), 6);
}

static ALLOWED: AttachedKey<bool> = AttachedKey::new("Allowed", Affects::RENDER, Inheritance::Combine(inherit::and), || false);

// A combining key read on a node that doesn't hold it gives what holding it would: the key's
// default combined with the nearest ancestor's value.
#[test]
fn combining_read_agrees_with_holding() {
  let mut tree = UiTree::new();
  let [root, _, leaf] = chain(&mut tree);
  tree.attached_property(root, &ALLOWED).set(true);

  let read = tree.attached(leaf, &ALLOWED);

  assert_eq!(read, tree.attached_property(leaf, &ALLOWED).get());
  assert!(!read);
}

// Setting an attached key on a node whose field shares the key writes that field, rather than a
// second property reads would ignore.
#[test]
fn attached_property_on_a_field_owner_is_the_field() {
  let mut tree = UiTree::new();
  let chrome = Property::new(CHROME.key(), 6);
  let node = tree.alloc(Box::new(Themed { chrome }));

  tree.attached_property(node, &CHROME).set(9);

  assert_eq!(chrome.get(), 9);
  assert_eq!(tree.attached(node, &CHROME), 9);
  assert_eq!(tree.attached_values.count(node), 0);
}
