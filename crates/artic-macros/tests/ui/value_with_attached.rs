use artic_macros::UiType;

#[derive(Debug, Clone, PartialEq, Default, UiType)]
#[ui(value, attached(name = "Row", ty = usize, target = children))]
struct Stop;

fn main() {}
