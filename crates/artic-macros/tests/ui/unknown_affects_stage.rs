use artic_core::property::Property;
use artic_macros::UiType;

#[derive(Debug, UiType)]
struct Panel {
  #[ui(affects = layut)]
  width: Property<f32>,
}

fn main() {}
