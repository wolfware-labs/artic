use artic_core::property::Property;
use artic_macros::{UiProperties, UiType};

#[derive(Debug, UiProperties)]
struct Common {
  #[ui]
  margin: Property<i32>,
}

#[derive(Debug, UiType)]
#[ui(namespace = "https://artic.rs/test")]
struct Widget {
  #[ui(flatten, content)]
  common: Common,
}

fn main() {}
