use artic_macros::element;

#[element]
#[derive(Debug)]
struct Key {
  #[element(status)]
  presses: u32,
}

fn main() {}
