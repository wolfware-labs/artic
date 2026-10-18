use artic_macros::element;

#[element]
#[derive(Debug)]
#[ui(namespace = "https://artic.rs/test")]
struct Pair(u8, u8);

fn main() {}
