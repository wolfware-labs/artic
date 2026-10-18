use artic_core::event::Event;
use artic_core::property::Property;
use artic_macros::UiType;

#[derive(Debug, UiType)]
#[ui(namespace = "https://artic.rs/ui", value_provider(positional(key, changed)))]
struct Localize {
  #[ui]
  key: Property<String>,
  #[ui]
  changed: Event<()>,
}

fn main() {}
