use artic_core::property::Property;
use artic_macros::{UiProperties, UiType};

#[derive(Debug, UiProperties)]
struct Labeled {
  #[ui(content)]
  label: Property<String>,
}

#[derive(Debug, UiType)]
struct Tag {
  #[ui(flatten)]
  labeled: Labeled,
  #[ui(content)]
  extra: Property<String>,
}

fn main() {}
