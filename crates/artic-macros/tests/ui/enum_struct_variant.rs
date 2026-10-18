use artic_macros::UiEnum;

#[derive(UiEnum)]
enum Shape {
  Circle { radius: f32 },
  Empty,
}

fn main() {}
