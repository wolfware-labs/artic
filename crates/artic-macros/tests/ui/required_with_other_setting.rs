use artic_macros::UiType;

#[derive(Debug, UiType)]
struct Login {
  #[ui(required, name = "Session")]
  session: u32,
}

fn main() {}
