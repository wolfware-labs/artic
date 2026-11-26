use artic_core::property::Property;
use artic_core::tree::{NodeId, UiTree};
use artic_macros::UiType;
use artic_registry::ValueError;

struct Ops<V> {
  take: fn(&mut UiTree, NodeId, NodeId, &'static str) -> Result<V, ValueError>,
}

trait Lookalike {
  fn object_ops<V: Default>(&self) -> Ops<V> {
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

fn main() {}
