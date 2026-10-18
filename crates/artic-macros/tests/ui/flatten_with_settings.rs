use artic_core::property::Property;
use artic_macros::{UiProperties, UiType};

#[derive(Debug, UiProperties)]
struct Common {
  #[ui]
  margin: Property<i32>,
}

#[derive(Debug, UiType)]
struct Widget {
  #[ui(flatten, name = "Base")]
  common: Common,
}

fn main() {}
