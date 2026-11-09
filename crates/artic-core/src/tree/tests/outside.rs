use super::nodes;
use crate::tree::outside::{Outside, Place};
use crate::tree::{TreeKind, UiTree};
use pretty_assertions::assert_eq;

// A node parked again in the same generation, as one cleared and marked again is, comes back once.
#[test]
fn a_node_parked_twice_comes_back_once() {
  let mut tree = UiTree::new();
  let [root, node] = nodes(&mut tree);
  let mut outside = Outside::new();
  for _ in 0..2 {
    let (_, mut take) = outside.begin(0, (TreeKind::Visual, TreeKind::Visual, root));
    assert_eq!(outside.place(&mut take, node, || false), Place::Parked);
    outside.end(take);
  }

  let (requeue, _) = outside.begin(1, (TreeKind::Visual, TreeKind::Visual, root));
  assert_eq!(requeue, [node]);
}
