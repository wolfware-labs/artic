// A trait in scope that would take any staged object as a default value changes nothing: the
// generated code names each value's object route by its full path, so a property not taken from
// objects still takes the staged object out of the tree and refuses it.
#![allow(dead_code)]

use artic_core::property::{Precedence, Property};
use artic_core::tree::{NodeId, UiTree};
use artic_macros::UiType;
use artic_registry::{UiType as _, ValueError};

struct Ops<V> {
  take: fn(&mut UiTree, NodeId, NodeId, &'static str) -> Result<V, ValueError>,
}

trait Lookalike {
  fn object_ops<V: Default>() -> Ops<V> {
    Ops {
      take: |_, _, _, _| Ok(V::default()),
    }
  }
}

impl<T> Lookalike for T {}

#[derive(Debug, UiType)]
struct Card {
  #[ui(default = 1.0)]
  width: Property<f32>,
}

fn main() {
  let mut tree = UiTree::new();
  let card = tree.alloc(Box::new(Card::default()));
  let staged = tree.alloc(Box::new(Card::default()));
  let width = Card::TYPE_INFO.property("Width").expect("a property").info.access.expect("thunks");

  let refused = (width.set_object)(&mut tree, card, staged, Precedence::Local);

  assert_eq!(
    refused,
    Err(ValueError::WrongType {
      expected: "f32",
      found: "an object",
    })
  );
  assert!(!tree.contains(staged));
  assert_eq!(tree.get_as::<Card>(card).map(|card| card.width.get()), Some(1.0));
}
