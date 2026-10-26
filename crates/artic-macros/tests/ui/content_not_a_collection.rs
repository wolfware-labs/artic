use artic_macros::UiType;

#[derive(Debug, UiType)]
struct Label {
  #[ui(content)]
  text: String,
}

fn main() {}
