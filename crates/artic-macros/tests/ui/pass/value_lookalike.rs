// A trait in scope with the routes' method names, implemented for every type, changes nothing:
// the generated code names each value's route by its full path, so the real conversions run.
#![allow(dead_code)]

use artic_core::property::{Precedence, Property};
use artic_core::tree::{NodeId, UiTree};
use artic_macros::UiType;
use artic_registry::{UiType as _, ValueError};

struct Ops;

trait Lookalike {
  fn value_ops() -> Ops {
    Ops
  }

  fn plain_value_ops() -> Ops {
    Ops
  }

  fn object_ops() -> Ops {
    Ops
  }

  fn literal_ops() -> Ops {
    Ops
  }
}

impl<T> Lookalike for T {}

#[derive(Debug, Clone, Default, PartialEq, UiType)]
#[ui(value)]
struct Needle {
  #[ui]
  length: f32,
}

#[derive(Debug, UiType)]
struct Gauge {
  #[ui]
  width: Property<f32>,
  #[ui]
  needle: Property<Option<Needle>>,
  #[ui]
  child: Property<Option<NodeId>>,
}

fn access(name: &str) -> &'static artic_registry::PropertyAccess {
  let info = Gauge::TYPE_INFO.property(name).expect("a property").info;
  info.access.expect("the access feature is on")
}

fn main() {
  let mut tree = UiTree::new();
  let gauge = tree.alloc(Box::new(Gauge::default()));
  let child = tree.alloc(Box::new(Gauge::default()));

  (access("Width").set_text)(&mut tree, gauge, "2.5", Precedence::Local).expect("a number");
  (access("Child").set_node)(&mut tree, gauge, child, Precedence::Local).expect("a node");
  let refused = (access("Width").set_node)(&mut tree, gauge, child, Precedence::Local);

  let needle = (Needle::TYPE_INFO.new.expect("a default"))(&mut tree);
  let length = Needle::TYPE_INFO.property("Length").expect("a field").info.access.expect("thunks");
  (length.set_text)(&mut tree, needle, "4", Precedence::Local).expect("a number");
  (access("Needle").set_object)(&mut tree, gauge, needle, Precedence::Local).expect("a needle");

  let gauge = tree.get_as::<Gauge>(gauge).expect("a Gauge");
  assert_eq!(gauge.width.get(), 2.5);
  assert_eq!(gauge.child.get(), Some(child));
  assert_eq!(gauge.needle.get(), Some(Needle { length: 4.0 }));
  assert_eq!(
    refused,
    Err(ValueError::WrongType {
      expected: "f32",
      found: "an element",
    })
  );
}
