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

// A pass over an empty queue is idle when its root was seen in the generation or nothing is
// parked off the queue, and not for a new root or a new generation while a node is parked.
#[test]
fn idle_only_when_seen_or_nothing_parked() {
  let mut tree = UiTree::new();
  let [first, second, node] = nodes(&mut tree);
  let key = |root| (TreeKind::Visual, TreeKind::Visual, root);
  let mut outside = Outside::new();
  assert!(outside.idle(0, key(first)));
  let (_, mut take) = outside.begin(0, key(first));
  assert_eq!(outside.place(&mut take, node, || false), Place::Parked);
  outside.end(take);

  assert_eq!(
    [
      outside.idle(0, key(first)),
      outside.idle(0, key(second)),
      outside.idle(1, key(first))
    ],
    [true, false, false]
  );
}
