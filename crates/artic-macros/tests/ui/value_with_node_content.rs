use artic_element::Children;
use artic_macros::UiType;

#[derive(Debug, Clone, Default, UiType)]
#[ui(value)]
struct Holder {
  #[ui(content)]
  items: Children,
}

fn main() {}
