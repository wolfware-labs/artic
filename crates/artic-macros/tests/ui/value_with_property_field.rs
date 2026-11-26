use artic_core::property::Property;
use artic_macros::UiType;

#[derive(Debug, UiType)]
#[ui(value)]
struct Stop {
  #[ui]
  offset: Property<f32>,
}

fn main() {}
