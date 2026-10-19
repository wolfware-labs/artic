use artic_core::property::Property;
use artic_macros::{UiProperties, UiType};

#[derive(Debug, UiProperties)]
struct Kind {
  #[ui]
  r#type: Property<String>,
}

#[derive(Debug, UiType)]
struct Card {
  #[ui(flatten)]
  kind: Kind,
  #[ui]
  r#type: Property<String>,
}

fn main() {}
