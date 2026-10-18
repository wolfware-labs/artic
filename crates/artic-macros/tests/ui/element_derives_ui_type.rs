use artic_macros::{UiType, element};

#[element]
#[derive(Debug, UiType)]
#[ui(namespace = "https://artic.rs/test")]
struct Key {}

fn main() {}
