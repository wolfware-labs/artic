use artic_core::property::Property;
use artic_macros::UiProperties;

#[derive(Debug, UiProperties)]
struct Spacing {
  #[ui(required)]
  gap: Property<f32>,
}

fn main() {}
