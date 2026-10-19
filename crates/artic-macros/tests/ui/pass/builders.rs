// A number literal must infer the property's type, so warnings are errors here.
#![deny(warnings)]

use artic_core::property::{Precedence, Property};
use artic_core::tree::UiTree;
use artic_macros::UiType;

#[derive(Debug, UiType)]
struct Card {
  #[ui(default = 1.0)]
  width: Property<f32>,
  #[ui]
  title: Property<Option<String>>,
}

fn main() {
  let mut tree = UiTree::new();

  let card = Card::new().width(2.5).title("Hi").build(&mut tree);

  let card = card.get(&tree);
  assert_eq!(card.width.get(), 2.5);
  assert_eq!(card.title.get().as_deref(), Some("Hi"));
  assert_eq!(card.width.effective_source(), Precedence::Local);
}
