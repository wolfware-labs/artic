// What an app's own code writes is at the facade's root: its elements, their value types and the
// reactive state of its code types.

use artic::{
  Element, ElementImpl, ElementKind, Handle, Memo, NodeId, Property, Signal, UiEnum, UiTree, UiType, effect, element,
};
use std::cell::Cell;
use std::rc::Rc;

/// How loud a badge is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, UiEnum)]
enum Tone {
  #[default]
  Calm,
  Loud,
}

/// An app's own element, made with the root's attribute.
#[element]
#[derive(Debug)]
struct Badge {
  /// How loud it is.
  #[ui(affects = render)]
  pub tone: Property<Tone>,
}

impl ElementImpl for Badge {}

/// Accepts an element that is its own kind.
fn own_kind<T: Element + ElementKind<Kind = T>>() {}

// An element made with the root's items builds into a tree, and its value type parses from text.
#[test]
fn root_items_make_an_element() {
  let mut tree = UiTree::new();

  let badge: Handle<Badge> = Badge::new().tone(Tone::Loud).build(&mut tree);
  let node: NodeId = badge.id();

  own_kind::<Badge>();
  assert_eq!(badge.get(&tree).tone.get(), Tone::Loud);
  assert_eq!(<Badge as UiType>::TYPE_INFO.name, "Badge");
  assert!(tree.contains(node));
  assert_eq!("Calm".parse(), Ok(Tone::Calm));
}

// A signal, a memo over it and an effect reading the memo are reachable from the root.
#[test]
fn root_items_hold_reactive_state() {
  let count = Signal::new(1);
  let doubled = Memo::new(move || count.get() * 2);
  let seen = Rc::new(Cell::new(0));
  let sink = Rc::clone(&seen);
  effect(move || sink.set(doubled.get()));

  count.set(2);
  artic::core::reactive::flush_effects();

  assert_eq!(seen.get(), 4);
}
