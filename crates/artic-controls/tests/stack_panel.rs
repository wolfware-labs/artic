// StackPanel lines its children up along its orientation, with spacing between the ones that take
// part in layout.

use artic_controls::StackPanel;
use artic_core::tree::{NodeId, UiTree};
use artic_element::{AccessCx, ElementTypes, Visibility};
use artic_layout::geometry::{Rect, Size};
use artic_layout::{LayoutNodes, Layoutable, Orientation};
use artic_registry::{ContentInfo, UiType};
use artic_test::layout::{Boxes, Fixed, Headless, Invariants, strategy};
use proptest::prelude::*;

/// Real elements first, then the shared test leaves.
struct Nodes(ElementTypes);

impl LayoutNodes for Nodes {
  fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable> {
    self.0.layoutable(tree, node).or_else(|| Boxes.layoutable(tree, node))
  }
}

const WINDOW: Size = Size::new(200.0, 300.0);

fn layout() -> Headless<Nodes> {
  Headless::new(Nodes(ElementTypes::from_link_time()))
}

fn stack(orientation: Orientation, spacing: f32) -> StackPanel {
  let stack = StackPanel::default();
  stack.orientation.set(orientation);
  stack.spacing.set(spacing);
  stack
}

// A vertical stack asks for the widest child and the heights plus spacing between them.
#[test]
fn stack_vertical_measure() {
  let mut layout = layout();
  let a = layout.add("a", Fixed::new(200.0, 40.0));
  let c = layout.add("c", Fixed::new(120.0, 30.0));
  let root = layout.parent("stack", stack(Orientation::Vertical, 5.0), &[a, c]);

  layout.run(root, WINDOW, 1.0);

  layout.assert_desired(root, Size::new(200.0, 75.0));
}

// A horizontal stack swaps the axes.
#[test]
fn stack_horizontal_measure() {
  let mut layout = layout();
  let a = layout.add("a", Fixed::new(40.0, 20.0));
  let c = layout.add("c", Fixed::new(30.0, 50.0));
  let root = layout.parent("stack", stack(Orientation::Horizontal, 5.0), &[a, c]);

  layout.run(root, WINDOW, 1.0);

  layout.assert_desired(root, Size::new(75.0, 50.0));
}

// A collapsed child gets no spacing, as if it weren't there.
#[test]
fn stack_spacing_skips_collapsed() {
  let mut layout = layout();
  let a = layout.add("a", Fixed::new(200.0, 40.0));
  let hidden = StackPanel::default();
  hidden.appearance.visibility.set(Visibility::Collapsed);
  let b = layout.add("b", hidden);
  let c = layout.add("c", Fixed::new(120.0, 30.0));
  let root = layout.parent("stack", stack(Orientation::Vertical, 5.0), &[a, b, c]);

  layout.run(root, WINDOW, 1.0);

  layout.assert_desired(root, Size::new(200.0, 75.0));
}

// Arrange hands out slots one after another, as long as each child asked and as wide as the panel.
#[test]
fn stack_arrange_positions_children() {
  for (orientation, a_rect, c_rect) in [
    (
      Orientation::Vertical,
      Rect::new(0.0, 0.0, 200.0, 40.0),
      Rect::new(0.0, 45.0, 200.0, 30.0),
    ),
    (
      Orientation::Horizontal,
      Rect::new(0.0, 0.0, 40.0, 300.0),
      Rect::new(45.0, 0.0, 30.0, 300.0),
    ),
  ] {
    let mut layout = layout();
    let a = layout.add("a", Fixed::new(40.0, 40.0));
    let c = layout.add("c", Fixed::new(30.0, 30.0));
    let root = layout.parent("stack", stack(orientation, 5.0), &[a, c]);

    layout.run(root, WINDOW, 1.0);

    layout.assert_rect(a, a_rect);
    layout.assert_rect(c, c_rect);
  }
}

// Negative, NaN and infinite spacing count as none.
#[test]
fn stack_spacing_ignores_invalid_values() {
  for spacing in [-5.0, f32::NAN, f32::INFINITY] {
    let mut layout = layout();
    let a = layout.add("a", Fixed::new(10.0, 10.0));
    let c = layout.add("c", Fixed::new(10.0, 10.0));
    let root = layout.parent("stack", stack(Orientation::Vertical, spacing), &[a, c]);

    layout.run(root, WINDOW, 1.0);

    layout.assert_desired(root, Size::new(10.0, 20.0));
  }
}

// A stack with no children asks for nothing.
#[test]
fn empty_stack_is_zero() {
  let mut layout = layout();
  let root = layout.add("stack", StackPanel::default());

  layout.run(root, Size::new(f32::INFINITY, f32::INFINITY), 1.0);

  layout.assert_desired(root, Size::ZERO);
}

// A stack whose children are all collapsed is as empty as one with no children.
#[test]
fn all_collapsed_children_give_an_empty_stack() {
  let mut layout = layout();
  let hidden = StackPanel::default();
  hidden.appearance.visibility.set(Visibility::Collapsed);
  let b = layout.add("b", hidden);
  let root = layout.parent("stack", stack(Orientation::Vertical, 5.0), &[b]);

  layout.run(root, Size::new(f32::INFINITY, f32::INFINITY), 1.0);

  layout.assert_desired(root, Size::ZERO);
}

// With no limit across, the stack asks for its widest child.
#[test]
fn infinite_cross_axis_takes_the_widest_child() {
  let mut layout = layout();
  let a = layout.add("a", Fixed::new(80.0, 10.0));
  let c = layout.add("c", Fixed::new(120.0, 10.0));
  let root = layout.parent("stack", stack(Orientation::Vertical, 0.0), &[a, c]);

  layout.run(root, Size::new(f32::INFINITY, f32::INFINITY), 1.0);

  layout.assert_desired(root, Size::new(120.0, 20.0));
}

// A huge spacing keeps every size and rectangle finite.
#[test]
fn huge_spacing_stays_finite() {
  let mut layout = layout();
  let a = layout.add("a", Fixed::new(10.0, 10.0));
  let c = layout.add("c", Fixed::new(10.0, 10.0));
  let root = layout.parent("stack", stack(Orientation::Vertical, f32::MAX), &[a, c]);

  layout.run(root, WINDOW, 1.0);

  let rect = layout.manager().rect(c).expect("arranged");
  assert!(
    [rect.x, rect.y, rect.width, rect.height]
      .iter()
      .all(|value| value.is_finite()),
    "{rect:?}"
  );
}

// The children field is the type's content.
#[test]
fn stack_children_are_its_content() {
  assert!(matches!(
    <StackPanel as UiType>::TYPE_INFO.content,
    Some(ContentInfo::Collection { field: "children", .. })
  ));
}

// A stack panel groups its children for accessibility, with no meaning of its own.
#[test]
fn stack_panel_accesskit_node() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(StackPanel::default()));
  let types = ElementTypes::from_link_time();
  let element = types.element(tree.get(node).expect("a node")).expect("an element");

  let access = element.accessibility(&AccessCx::new(&tree, node, Rect::new(0.0, 0.0, 10.0, 10.0)));

  assert_eq!(access.role(), accesskit::Role::GenericContainer);
}

proptest! {
  // Any children, any available size and scale, both orientations: the answer is the same from a
  // fresh manager.
  #[test]
  fn stack_invariants_hold(
    sizes in proptest::collection::vec((0.0..300.0f32, 0.0..300.0f32), 0..6),
    available in strategy::available(),
    scale in strategy::scale(),
    horizontal in any::<bool>(),
  ) {
    let mut layout = layout();
    let children: Vec<_> = sizes
      .iter()
      .enumerate()
      .map(|(index, (width, height))| layout.add(&format!("child {index}"), Fixed::new(*width, *height)))
      .collect();
    let orientation = if horizontal { Orientation::Horizontal } else { Orientation::Vertical };
    let root = layout.parent("stack", stack(orientation, 4.0), &children);

    Invariants::default().assert(&mut layout, root, available, scale);
  }
}
