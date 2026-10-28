mod attached;
mod both;
mod dirty;
mod free;
mod handle;
mod inheritance;
mod links;
mod objects;
mod structure;
mod wiring;

use crate::Object;
use crate::property::PropertyVisitor;
use crate::tree::{NodeId, UiTree};

#[derive(Debug, PartialEq, Eq)]
struct Control;

impl Object for Control {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

/// Allocates `N` nodes and returns their ids, in order.
fn nodes<const N: usize>(tree: &mut UiTree) -> [NodeId; N] {
  std::array::from_fn(|_| tree.alloc(Box::new(Control)))
}
