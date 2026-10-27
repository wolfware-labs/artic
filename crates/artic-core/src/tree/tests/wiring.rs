#![cfg(debug_assertions)]

use crate::Object;
use crate::property::{Affects, Inheritance, Property, PropertyKey, PropertyVisitor};
use crate::tree::UiTree;
use std::panic::{AssertUnwindSafe, catch_unwind};

static WIDTH: PropertyKey<i32> = PropertyKey::new("Width", Affects::LAYOUT, Inheritance::None);
static HEIGHT: PropertyKey<i32> = PropertyKey::new("Height", Affects::LAYOUT, Inheritance::None);

#[derive(Debug)]
struct Panel {
  width: Property<i32>,
  height: Property<i32>,
  items: Vec<i32>,
}

impl Panel {
  fn new() -> Panel {
    Panel {
      width: Property::new(&WIDTH, 0),
      height: Property::new(&HEIGHT, 0),
      items: Vec::new(),
    }
  }
}

impl Object for Panel {
  fn visit_properties(&self, visitor: &mut PropertyVisitor<'_>) {
    visitor.visit(&self.width);
    visitor.visit(&self.height);
  }
}

// A property field replaced through `get_as_mut` isn't wired to the node, so in debug builds the
// next `take_pending` reports it.
#[test]
#[should_panic(expected = "a property field was replaced through get_mut or get_as_mut")]
fn a_replaced_field_panics_at_take_pending() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Panel::new()));
  tree.get_as_mut::<Panel>(node).unwrap().width = Property::new(&WIDTH, 5);

  let _ = tree.take_pending();
}

// A field overwritten with a copy of a sibling's handle still names a property of the node, but
// the one it held is gone from the node's list and would never be disposed.
#[test]
#[should_panic(expected = "a property field was replaced through get_mut or get_as_mut")]
fn a_field_copied_from_a_sibling_panics() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Panel::new()));
  let panel = tree.get_as_mut::<Panel>(node).unwrap();
  panel.width = panel.height;

  let _ = tree.take_pending();
}

// A field overwritten with the handle of a freed node's property loses the one it held too.
#[test]
#[should_panic(expected = "a property field was replaced through get_mut or get_as_mut")]
fn a_field_given_a_disposed_handle_panics() {
  let mut tree = UiTree::new();
  let [node, other] = [(); 2].map(|_| tree.alloc(Box::new(Panel::new())));
  let disposed = tree.get_as::<Panel>(other).unwrap().width;
  tree.free(other);
  tree.get_as_mut::<Panel>(node).unwrap().width = disposed;

  let _ = tree.take_pending();
}

// `free` reports a replaced field too, before it frees anything.
#[test]
fn a_replaced_field_panics_at_free_before_freeing() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Panel::new()));
  tree.get_as_mut::<Panel>(node).unwrap().width = Property::new(&WIDTH, 5);

  let freed = catch_unwind(AssertUnwindSafe(|| tree.free(node)));

  assert!(freed.is_err());
  assert_eq!(tree.len(), 1);
}

// A caught report doesn't forget the node: the next `free` reports it again instead of freeing it
// and leaving the replaced property undisposed.
#[test]
fn a_caught_report_is_raised_again() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Panel::new()));
  tree.get_as_mut::<Panel>(node).unwrap().width = Property::new(&WIDTH, 5);
  let _ = catch_unwind(AssertUnwindSafe(|| tree.take_pending()));

  let freed = catch_unwind(AssertUnwindSafe(|| tree.free(node)));

  assert!(freed.is_err());
  assert_eq!(tree.len(), 1);
}

// Changing other fields, or writing a property through `set`, passes.
#[test]
fn touching_other_fields_passes() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Panel::new()));
  let sized = tree.get_as_mut::<Panel>(node).unwrap();
  sized.items.push(1);
  sized.width.set(5);

  assert!(tree.take_pending());
  tree.free(node);
}

// `get_mut` on a stale id hands out nothing, so it leaves nothing to check.
#[test]
fn get_mut_on_a_stale_id_records_nothing() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Panel::new()));
  tree.free(node);

  assert!(tree.get_mut(node).is_none());
  let _ = tree.take_pending();
}
