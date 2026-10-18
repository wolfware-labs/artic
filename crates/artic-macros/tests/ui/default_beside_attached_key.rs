use artic_core::property::Property;
use artic_macros::UiType;

#[derive(Debug, UiType)]
#[ui(namespace = "https://artic.rs/ui")]
#[ui(attached(name = "Size", ty = f32, default = 14.0, target = any))]
struct Text;

#[derive(Debug, UiType)]
#[ui(namespace = "https://artic.rs/ui")]
struct Label {
  #[ui(key = Text::SIZE, default = 12.0)]
  size: Property<f32>,
}

fn main() {}
