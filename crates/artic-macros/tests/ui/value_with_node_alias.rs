use artic_core::tree::NodeId;
use artic_macros::UiType;

type Sample = NodeId;

#[derive(Debug, Clone, Default, UiType)]
#[ui(value)]
struct Swatch {
  #[ui]
  sample: Option<Sample>,
}

fn main() {}
