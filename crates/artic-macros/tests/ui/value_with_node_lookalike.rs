use artic_core::tree::NodeId;
use artic_macros::UiType;
use artic_registry::ValueError;

type Sample = NodeId;

struct Ops {
  parse: fn(&str, &'static str) -> Result<NodeId, ValueError>,
  node: fn(NodeId, &'static str) -> Result<NodeId, ValueError>,
}

trait Lookalike {
  fn value_must_parse_from_text_and_implement_torust_or_derive_uitype(&self) -> Ops {
    Ops {
      parse: |_, _| Err(ValueError::WrongNode),
      node: |node, _| Ok(node),
    }
  }
}

impl<T> Lookalike for T {}

#[derive(Debug, Clone, Default, UiType)]
#[ui(value)]
struct Swatch {
  #[ui]
  sample: Option<Sample>,
}

fn main() {}
