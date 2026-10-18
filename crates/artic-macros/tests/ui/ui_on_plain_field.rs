use artic_macros::UiType;

#[derive(Debug, UiType)]
#[ui(namespace = "https://artic.rs/ui")]
struct Label {
  #[ui]
  text: String,
}

fn main() {}
