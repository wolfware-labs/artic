#![deny(unused_must_use)]

use artic_core::property::Property;
use artic_core::tree::UiTree;
use artic_macros::UiType;

#[derive(Debug, UiType)]
struct Card {
  #[ui]
  width: Property<f32>,
}

fn main() {
  let mut tree = UiTree::new();
  Card::new().width(1.0).build(&mut tree);
}
