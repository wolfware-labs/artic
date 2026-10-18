use artic_macros::UiType;

#[derive(Debug, UiType)]
#[ui(namespace = "https://artic.rs/ui")]
#[ui(attached(name = "Row", ty = usize, target = children))]
#[ui(attached(name = "Row", ty = i32, target = children))]
struct Grid;

fn main() {}
