// The prelude alone brings the group methods into scope, so a view builds controls with the
// properties every element has.

use artic::prelude::*;
use artic_controls::{Border, StackPanel};
use artic_core::tree::UiTree;
use artic_layout::geometry::Thickness;
use artic_render::Color;

// Group methods from the prelude set the shared properties, beside a control's own.
#[test]
fn prelude_brings_group_methods() {
  let mut tree = UiTree::new();

  let border: Handle<Border> = Border::new()
    .margin(4.0)
    .width(100.0)
    .classes(["primary"])
    .padding(2.0)
    .build(&mut tree);
  let panel = StackPanel::new()
    .background(Color::rgb(0, 0, 0))
    .children([border])
    .build(&mut tree);

  let node = border.get(&tree);
  assert_eq!(node.layout.margin.get(), Thickness::uniform(4.0));
  assert_eq!(node.layout.width.get(), Some(100.0));
  assert!(node.identity.classes.get().contains("primary"));
  assert!(panel.get(&tree).panel.background.get().is_some());
}
