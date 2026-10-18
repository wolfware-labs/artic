use artic_macros::element;

#[element]
#[derive(Debug)]
struct Wrapper<T> {
  value: T,
}

fn main() {}
