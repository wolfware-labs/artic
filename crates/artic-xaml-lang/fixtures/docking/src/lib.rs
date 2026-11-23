//! A test-only owner of an attached property only its children may carry, since no artic control
//! declares one yet (`Grid` comes with #67).

use artic_element::Children;
use artic_macros::UiType;

/// Holds children, each placed in the slot its attached `Dock.Slot` gives.
#[derive(Debug, UiType)]
#[ui(attached(name = "Slot", ty = usize, target = children))]
pub struct Dock {
  /// The children.
  #[ui(content)]
  pub children: Children,
}
