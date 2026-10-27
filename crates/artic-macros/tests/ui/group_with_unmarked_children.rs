use artic_core::property::Property;
use artic_element::Children;
use artic_macros::UiProperties;

#[derive(Debug, UiProperties)]
struct Common {
  #[ui]
  width: Property<f32>,
  items: Children,
}

fn main() {}
