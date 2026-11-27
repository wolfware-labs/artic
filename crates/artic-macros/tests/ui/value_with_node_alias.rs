#![allow(dead_code)]

use artic_core::tree::NodeId;
use artic_macros::UiType;

type Sample = NodeId;

struct Mine;

trait Lookalike {
  fn value_ops() -> Mine {
    Mine
  }

  fn plain_value_ops() -> Mine {
    Mine
  }
}

impl<T> Lookalike for T {}

trait LookalikeMethods {
  fn value_ops(&self) -> Mine {
    Mine
  }

  fn plain_value_ops(&self) -> Mine {
    Mine
  }
}

impl<T> LookalikeMethods for T {}

#[derive(Debug, Clone, Default, UiType)]
#[ui(value)]
struct Swatch {
  #[ui]
  sample: Option<Sample>,
}

fn main() {}
