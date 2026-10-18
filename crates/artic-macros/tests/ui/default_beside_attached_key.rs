use artic_core::property::Property;
use artic_macros::XamlType;

#[derive(Debug, XamlType)]
#[xaml(namespace = "https://artic.rs/ui")]
#[xaml(attached(name = "Size", ty = f32, default = 14.0, target = any))]
struct Text;

#[derive(Debug, XamlType)]
#[xaml(namespace = "https://artic.rs/ui")]
struct Label {
  #[xaml(key = Text::SIZE, default = 12.0)]
  size: Property<f32>,
}

fn main() {}
