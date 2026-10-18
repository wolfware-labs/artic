use artic_core::property::Property;
use artic_macros::UiType;

#[derive(Debug, UiType)]
#[ui(override_defaults)]
struct Gauge {
  #[ui]
  value: Property<i32>,
}

fn main() {}
