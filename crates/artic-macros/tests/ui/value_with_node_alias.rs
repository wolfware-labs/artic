#![allow(unused_imports)]

use artic_core::tree::NodeId;
use artic_macros::UiType;
use artic_registry::__private::{ViaChild, ViaFromStr, ViaUiType};

type Sample = NodeId;

#[derive(Debug, Clone, Default, UiType)]
#[ui(value)]
struct Swatch {
  #[ui]
  sample: Option<Sample>,
}

fn main() {}
