use artic_core::property::Property;
use artic_macros::UiType;

#[derive(Debug, UiType)]
struct Card {
  #[ui]
  width: Property<f32>,
  #[ui]
  title: Property<String>,
}

fn main() {
  let _ = Card::new().width("wide");
  let _ = Card::new().title(5);
}
