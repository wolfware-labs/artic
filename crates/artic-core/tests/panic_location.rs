// Integration tests for where the tree's stale-id panics point. This file is its own test binary,
// so the panic hook it sets reaches no other test.

use artic_core::Object;
use artic_core::property::{Affects, AttachedKey, Inheritance, PropertyVisitor};
use artic_core::tree::UiTree;
use std::panic::{self, AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

static ROW: AttachedKey<usize> = AttachedKey::new("Row", Affects::LAYOUT, Inheritance::None, || 0);

#[derive(Debug)]
struct Leaf;

impl Object for Leaf {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

// Reading or creating an attached value on a stale id panics at the caller's line, not inside
// the tree, so the report names the code that kept the id.
#[test]
fn attached_values_on_a_stale_id_panic_at_the_caller() {
  let files = Arc::new(Mutex::new(Vec::new()));
  let recorded = Arc::clone(&files);
  panic::set_hook(Box::new(move |info| {
    recorded
      .lock()
      .unwrap()
      .push(info.location().map(|location| location.file().to_owned()));
  }));
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Leaf));
  tree.free(node);

  let read = catch_unwind(AssertUnwindSafe(|| tree.attached(node, &ROW)));
  let created = catch_unwind(AssertUnwindSafe(|| tree.attached_property(node, &ROW)));

  let _ = panic::take_hook();
  assert!(read.is_err() && created.is_err());
  let here = Some(file!().to_owned());
  assert_eq!(*files.lock().unwrap(), [here.clone(), here]);
}
