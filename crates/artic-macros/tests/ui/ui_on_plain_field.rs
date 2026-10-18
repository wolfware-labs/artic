use artic_macros::UiType;

#[derive(Debug, UiType)]
struct Label {
  #[ui]
  text: String,
}

fn main() {}
