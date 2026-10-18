use artic_core::property::Property;
use artic_macros::UiType;

#[derive(Debug, UiType)]
#[ui(namespace = "https://artic.rs/ui", attached(name = "Row", ty = usize, target = children))]
struct Grid {
  #[ui]
  row: Property<usize>,
}

fn main() {}
