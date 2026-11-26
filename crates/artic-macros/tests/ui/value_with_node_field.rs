use artic_core::tree::NodeId;
use artic_macros::UiType;

#[derive(Debug, Clone, Default, UiType)]
#[ui(value)]
struct Swatch {
  #[ui]
  sample: Option<NodeId>,
}

fn main() {}
