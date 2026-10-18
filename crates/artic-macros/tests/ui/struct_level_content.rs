use artic_core::property::Property;
use artic_macros::UiType;

#[derive(Debug, UiType)]
#[ui(content = text)]
struct Label {
  #[ui]
  text: Property<String>,
}

fn main() {}
