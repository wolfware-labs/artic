use artic_core::property::Property;
use artic_macros::UiType;

#[derive(Debug, UiType)]
#[ui(value_provider(positional(key, key)))]
struct Localize {
  #[ui]
  key: Property<String>,
}

fn main() {}
