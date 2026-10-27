#![cfg(debug_assertions)]

use crate::Object;
use crate::property::{Affects, Inheritance, Property, PropertyKey, PropertyVisitor};
use crate::tree::UiTree;
use std::panic::{AssertUnwindSafe, catch_unwind};

static WIDTH: PropertyKey<i32> = PropertyKey::new("Width", Affects::LAYOUT, Inheritance::None);

#[derive(Debug)]
struct Panel {
  width: Property<i32>,
  items: Vec<i32>,
}

impl Panel {
  fn new() -> Panel {
    Panel {
      width: Property::new(&WIDTH, 0),
      items: Vec::new(),
    }
  }
}

impl Object for Panel {
  fn visit_properties(&self, visitor: &mut PropertyVisitor<'_>) {
    visitor.visit(&self.width);
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
