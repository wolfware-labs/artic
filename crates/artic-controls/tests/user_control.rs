// UserControl: a view's root, which describes itself as a plain container and fills its content
// into its box.

use artic_controls::UserControl;
use artic_core::tree::{TreeKind, UiTree};
use artic_element::{AccessCx, ElementTypes};
use artic_layout::geometry::{Rect, Size};
use artic_layout::{LayoutManager, PixelGrid};
use pretty_assertions::assert_eq;

// A UserControl is a plain container to accessibility.
#[test]
fn user_control_accesskit_node() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(UserControl::default()));
  let types = ElementTypes::from_link_time();

  let element = types
    .element(tree.get(node).expect("live"))
    .expect("UserControl is an element");
  let described = element.accessibility(&AccessCx::new(&tree, node, Rect::new(0.0, 0.0, 10.0, 10.0)));

  assert_eq!(described.role(), accesskit::Role::GenericContainer);
}

// A UserControl wants its content's size and fills its content into its box.
#[test]
fn user_control_lays_out_its_content() {
  let mut tree = UiTree::new();
  let view = tree.alloc(Box::new(UserControl::default()));
  let content = tree.alloc(Box::new(UserControl::default()));
  let inner = tree.get_as::<UserControl>(content).expect("a user control");
  inner.layout.width.set(Some(40.0));
  inner.layout.height.set(Some(30.0));
  tree.add_child(TreeKind::Visual, view, content);
  let mut layout = LayoutManager::default();

  layout.run(
    &mut tree,
    &ElementTypes::from_link_time(),
    view,
    Size::new(f32::INFINITY, f32::INFINITY),
    PixelGrid { scale: 1.0, snap: false },
    &mut artic_test::text::engine(),
  );

  assert_eq!(layout.desired(view), Some(Size::new(40.0, 30.0)));
  assert_eq!(layout.rect(content), Some(Rect::new(0.0, 0.0, 40.0, 30.0)));
}

// A user control takes one child as content, linked in both trees.
#[test]
fn user_control_builder_takes_one_child() {
  let mut tree = UiTree::new();
  let content = UserControl::new().build(&mut tree);

  let view = UserControl::new().child(content).build(&mut tree);

  for kind in TreeKind::ALL {
    assert_eq!(tree.children(kind, view.id()).collect::<Vec<_>>(), [content.id()]);
  }
}
