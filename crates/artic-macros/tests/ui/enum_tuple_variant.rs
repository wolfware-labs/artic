use artic_macros::UiEnum;

#[derive(UiEnum)]
enum Brush {
  Solid(u32),
  None,
}

fn main() {}
