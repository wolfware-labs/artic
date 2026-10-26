use artic_macros::UiType;

#[derive(Debug, UiType)]
struct Gradient {
  #[ui]
  stops: Vec<u8>,
}

fn main() {}
