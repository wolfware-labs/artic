use crate::Object;
use crate::property::{Affects, AnyProperty, AttachedKey, Inheritance, Precedence, Property, PropertyKey};
use crate::tree::{NodeId, TreeKind, UiTree};
use pretty_assertions::assert_eq;

static FONT_SIZE: PropertyKey<i32> = PropertyKey::new("FontSize", Affects::LAYOUT, Inheritance::None);
static FOREGROUND: PropertyKey<i32> = PropertyKey::new("Foreground", Affects::RENDER, Inheritance::None);

#[derive(Debug)]
struct Label {
  font_size: Property<i32>,
  foreground: Property<i32>,
}

impl Object for Label {
  fn visit_properties(&self, visit: &mut dyn FnMut(AnyProperty<'_>)) {
    visit(self.font_size.as_ref());
    visit(self.foreground.as_ref());
  }
}

/// Allocates a label, clears the dirty flags every new node starts with, and returns its id with
/// its font size and foreground properties.
fn clean_label(tree: &mut UiTree) -> (NodeId, Property<i32>, Property<i32>) {
  let font_size = Property::new(&FONT_SIZE, 12);
  let foreground = Property::new(&FOREGROUND, 0);
  let id = tree.alloc(Box::new(Label { font_size, foreground }));
  tree.clear_dirty(id, Affects::LAYOUT | Affects::RENDER);
  (id, font_size, foreground)
}

// A new node needs layout and rendering.
#[test]
fn new_node_starts_dirty() {
  let mut tree = UiTree::new();
  let id = tree.alloc(Box::new(Label {
    font_size: Property::new(&FONT_SIZE, 12),
    foreground: Property::new(&FOREGROUND, 0),
  }));

  assert_eq!(tree.dirty(id), Affects::LAYOUT | Affects::RENDER);
}

// Changing a property whose key affects layout marks its node layout-dirty.
#[test]
fn layout_affecting_change_sets_measure_dirty() {
  let mut tree = UiTree::new();
  let (id, font_size, _) = clean_label(&mut tree);

  font_size.set(24);

  assert_eq!(tree.dirty(id), Affects::LAYOUT);
}

// Changing a property whose key affects only rendering marks the node render-dirty, not
// layout-dirty.
#[test]
fn render_only_change_does_not_set_measure_dirty() {
  let mut tree = UiTree::new();
  let (id, _, foreground) = clean_label(&mut tree);

  foreground.set(0xff0000);

  assert_eq!(tree.dirty(id), Affects::RENDER);
}

// A write that doesn't change the effective value marks nothing.
#[test]
fn hidden_write_marks_nothing() {
  let mut tree = UiTree::new();
  let (id, font_size, _) = clean_label(&mut tree);
  font_size.set(24);
  tree.clear_dirty(id, Affects::LAYOUT);

  font_size.set_at(Precedence::StyleSetter, 30);
  font_size.set(24);

  assert_eq!(tree.dirty(id), Affects::NONE);
}

// `mark_dirty` adds stages and `clear_dirty` removes only the ones given.
#[test]
fn mark_and_clear_dirty_change_only_the_given_stages() {
  let mut tree = UiTree::new();
  let (id, _, _) = clean_label(&mut tree);

  tree.mark_dirty(id, Affects::LAYOUT | Affects::RENDER);
  tree.clear_dirty(id, Affects::LAYOUT);

  assert_eq!(tree.dirty(id), Affects::RENDER);
}

// A freed node's properties are freed with it, so a copy kept outside the tree can't mark the node
// that takes its slot: writing it panics.
#[test]
#[should_panic(expected = "signal used after it was disposed")]
fn freed_node_property_write_panics() {
  let mut tree = UiTree::new();
  let (old, font_size, _) = clean_label(&mut tree);
  tree.free(old);

  font_size.set(24);
}

// An attached value is freed with its node too.
#[test]
#[should_panic(expected = "signal used after it was disposed")]
fn freed_node_attached_write_panics() {
  let mut tree = UiTree::new();
  let (old, _, _) = clean_label(&mut tree);
  let row = tree.attached_property(old, &ROW);
  tree.free(old);

  row.set(2);
}

static NAME: PropertyKey<i32> = PropertyKey::new("Name", Affects::NONE, Inheritance::None);
static ROW: AttachedKey<i32> = AttachedKey::new("Row", Affects::LAYOUT, Inheritance::None, || 0);

#[derive(Debug)]
struct Named {
  name: Property<i32>,
}

impl Object for Named {
  fn visit_properties(&self, visit: &mut dyn FnMut(AnyProperty<'_>)) {
    visit(self.name.as_ref());
  }
}

// A new node needs a frame, so allocating it sets the tree's pending flag.
#[test]
fn new_node_sets_pending() {
  let mut tree = UiTree::new();
  assert!(!tree.take_pending());

  let _ = clean_label(&mut tree);

  assert!(tree.take_pending());
}

// A property write that marks its node dirty sets the pending flag, and taking it clears it.
#[test]
fn property_write_sets_pending() {
  let mut tree = UiTree::new();
  let (_, font_size, _) = clean_label(&mut tree);
  let _ = tree.take_pending();

  font_size.set(24);

  assert!(tree.take_pending());
  assert!(!tree.take_pending());
}

// `mark_dirty` sets the pending flag.
#[test]
fn mark_dirty_sets_pending() {
  let mut tree = UiTree::new();
  let (id, _, _) = clean_label(&mut tree);
  let _ = tree.take_pending();

  tree.mark_dirty(id, Affects::RENDER);

  assert!(tree.take_pending());
}

// Adding a visual child marks the parent for layout, which sets the pending flag.
#[test]
fn child_change_sets_pending() {
  let mut tree = UiTree::new();
  let (parent, _, _) = clean_label(&mut tree);
  let (child, _, _) = clean_label(&mut tree);
  let _ = tree.take_pending();

  tree.add_child(TreeKind::Visual, parent, child);

  assert!(tree.take_pending());
}

// A write to an attached property sets the pending flag like any other property.
#[test]
fn attached_property_write_sets_pending() {
  let mut tree = UiTree::new();
  let (id, _, _) = clean_label(&mut tree);
  let row = tree.attached_property(id, &ROW);
  let _ = tree.take_pending();

  row.set(2);

  assert!(tree.take_pending());
}

// A write that changes nothing, a change to a property affecting no stage, and clearing flags
// leave the pending flag unset.
#[test]
fn change_affecting_nothing_sets_no_pending() {
  let mut tree = UiTree::new();
  let (id, font_size, _) = clean_label(&mut tree);
  let name = Property::new(&NAME, 0);
  let named = tree.alloc(Box::new(Named { name }));
  font_size.set(24);
  let _ = tree.take_pending();

  font_size.set(24);
  name.set(1);
  tree.clear_dirty(id, Affects::LAYOUT);
  tree.clear_dirty(named, Affects::LAYOUT | Affects::RENDER);

  assert!(!tree.take_pending());
}
