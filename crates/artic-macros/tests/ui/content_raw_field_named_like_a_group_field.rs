use artic_core::property::Property;
use artic_element::Children;
use artic_macros::{UiProperties, UiType};

#[derive(Debug, UiProperties)]
struct Sizing {
  #[ui]
  width: Property<f32>,
}

#[derive(Debug, UiType)]
struct Card {
  #[ui(flatten)]
  sizing: Sizing,
  #[ui(content)]
  r#width: Children,
}

fn main() {}
