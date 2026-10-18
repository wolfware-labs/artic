use artic_core::event::Event;
use artic_macros::UiProperties;

#[derive(Debug, UiProperties)]
struct Common {
  #[ui]
  clicked: Event<()>,
}

fn main() {}
