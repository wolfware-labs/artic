use super::{Control, nodes};
use crate::tree::{NodeMap, UiTree};
use pretty_assertions::assert_eq;

// A value is found by the id it went in under, and not by a later node in the same slot.
#[test]
fn a_reused_slot_finds_nothing_until_set() {
  let mut tree = UiTree::new();
  let [old] = nodes(&mut tree);
  let mut map = NodeMap::new();
  map.insert(old, 1);
  tree.free(old);
  let new = tree.alloc(Box::new(Control));

  assert_eq!(map.get(new), None);
  assert_eq!(map.get(old), Some(&1));
}

// get_or_default inserts the default once and hands back the same value after.
#[test]
fn get_or_default_inserts_once() {
  let mut tree = UiTree::new();
  let [node] = nodes(&mut tree);
  let mut map = NodeMap::<u32>::new();
  *map.get_or_default(node) += 2;
  *map.get_or_default(node) += 3;

  assert_eq!(map.get(node), Some(&5));
}

// get_or_default replaces the value a freed node left in the slot.
#[test]
fn get_or_default_replaces_a_freed_nodes_value() {
  let mut tree = UiTree::new();
  let [old] = nodes(&mut tree);
  let mut map = NodeMap::<u32>::new();
  map.insert(old, 7);
  tree.free(old);
  let new = tree.alloc(Box::new(Control));

  assert_eq!(*map.get_or_default(new), 0);
  assert_eq!(map.get(old), None);
}

// retain drops exactly the entries it turns down and passes each its id.
#[test]
fn retain_drops_what_it_turns_down() {
  let mut tree = UiTree::new();
  let [a, b] = nodes(&mut tree);
  let mut map = NodeMap::new();
  map.insert(a, 1);
  map.insert(b, 2);

  map.retain(|node, _| node == b);

  assert_eq!((map.get(a), map.get(b)), (None, Some(&2)));
}

// get_mut and remove reach the value, and a removed value is gone.
#[test]
fn get_mut_and_remove_reach_the_value() {
  let mut tree = UiTree::new();
  let [node] = nodes(&mut tree);
  let mut map = NodeMap::new();
  map.insert(node, 1);
  *map.get_mut(node).unwrap() += 1;

  assert_eq!(map.remove(node), Some(2));
  assert_eq!(map.get(node), None);
}

// iter_mut reaches every value with its node.
#[test]
fn iter_mut_reaches_every_value_with_its_node() {
  let mut tree = UiTree::new();
  let [a, b] = nodes(&mut tree);
  let mut map = NodeMap::new();
  map.insert(a, 1);
  map.insert(b, 2);

  map
    .iter_mut()
    .for_each(|(node, value)| *value *= if node == a { 10 } else { 100 });

  assert_eq!((map.get(a), map.get(b)), (Some(&10), Some(&200)));
}

// insert hands back what the slot held: the node's own value, or a freed node's.
#[test]
fn insert_hands_back_what_the_slot_held() {
  let mut tree = UiTree::new();
  let [old] = nodes(&mut tree);
  let mut map = NodeMap::new();

  assert_eq!(map.insert(old, 1), None);
  assert_eq!(map.insert(old, 2), Some((old, 1)));
  tree.free(old);
  let new = tree.alloc(Box::new(Control));
  assert_eq!(map.insert(new, 3), Some((old, 2)));
}

// Removing or retaining away a value keeps every other value found by its node.
#[test]
fn removals_keep_the_other_values_found() {
  let mut tree = UiTree::new();
  let [a, b, c, d] = nodes(&mut tree);
  let mut map = NodeMap::new();
  for (value, node) in [a, b, c, d].into_iter().enumerate() {
    map.insert(node, value);
  }

  map.remove(a);
  map.retain(|node, _| node != c);

  assert_eq!(
    [a, b, c, d].map(|node| map.get(node).copied()),
    [None, Some(1), None, Some(3)]
  );
}
