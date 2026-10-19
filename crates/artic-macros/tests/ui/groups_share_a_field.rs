use artic_core::property::Property;
use artic_macros::{UiProperties, UiType};

#[derive(Debug, UiProperties)]
struct First {
  #[ui]
  width: Property<f32>,
}

#[derive(Debug, UiProperties)]
struct Second {
  #[ui]
  width: Property<f32>,
}

#[derive(Debug, UiType)]
struct Card {
  #[ui(flatten)]
  first: First,
  #[ui(flatten)]
  second: Second,
}

fn main() {}
