use artic_core::property::Property;
use artic_macros::UiType;

#[derive(Debug, UiType)]
struct Label {
  #[ui(content)]
  text: Property<String>,
  #[ui(content)]
  hint: Property<String>,
}

fn main() {}
