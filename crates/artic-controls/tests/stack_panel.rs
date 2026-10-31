// StackPanel lines its children up along its orientation, with spacing between the ones that take
// part in layout.

use artic_controls::StackPanel;
use artic_core::tree::{TreeKind, UiTree};
use artic_element::{AccessCx, ElementTypes, Visibility};
use artic_layout::Orientation;
use artic_layout::geometry::{Rect, Size};
use artic_registry::{ContentInfo, ContentMethod, UiType};
use artic_render::{Color, Command};
use artic_test::layout::{Fixed, Headless, Invariants, strategy};
use proptest::prelude::*;

const WINDOW: Size = Size::new(200.0, 300.0);

fn stack(orientation: Orientation, spacing: f32) -> StackPanel {
  let stack = StackPanel::default();
  stack.orientation.set(orientation);
  stack.spacing.set(spacing);
  stack
}

// A vertical stack asks for the widest child and the heights plus spacing between them.
#[test]
fn stack_vertical_measure() {
  let mut layout = Headless::new();
  let a = layout.add("a", Fixed::new(200.0, 40.0));
  let c = layout.add("c", Fixed::new(120.0, 30.0));
  let root = layout.parent("stack", stack(Orientation::Vertical, 5.0), &[a, c]);

  layout.run(root, WINDOW, 1.0);

  layout.assert_desired(root, Size::new(200.0, 75.0));
}

// Spacing goes between each pair of children: three children get it twice.
#[test]
fn spacing_goes_between_each_pair() {
  let mut layout = Headless::new();
  let a = layout.add("a", Fixed::new(10.0, 10.0));
  let b = layout.add("b", Fixed::new(10.0, 20.0));
  let c = layout.add("c", Fixed::new(10.0, 30.0));
  let root = layout.parent("stack", stack(Orientation::Vertical, 5.0), &[a, b, c]);

  layout.run(root, WINDOW, 1.0);

  layout.assert_desired(root, Size::new(10.0, 70.0));
}

// A horizontal stack swaps the axes.
#[test]
fn stack_horizontal_measure() {
  let mut layout = Headless::new();
  let a = layout.add("a", Fixed::new(40.0, 20.0));
  let c = layout.add("c", Fixed::new(30.0, 50.0));
  let root = layout.parent("stack", stack(Orientation::Horizontal, 5.0), &[a, c]);

  layout.run(root, WINDOW, 1.0);

  layout.assert_desired(root, Size::new(75.0, 50.0));
}

// A collapsed child gets no spacing, as if it weren't there.
#[test]
fn stack_spacing_skips_collapsed() {
  let mut layout = Headless::new();
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
    let mut layout = Headless::new();
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
    let mut layout = Headless::new();
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
  let mut layout = Headless::new();
  let root = layout.add("stack", StackPanel::default());

  layout.run(root, Size::new(f32::INFINITY, f32::INFINITY), 1.0);

  layout.assert_desired(root, Size::ZERO);
}

// A stack whose children are all collapsed is as empty as one with no children.
#[test]
fn all_collapsed_children_give_an_empty_stack() {
  let mut layout = Headless::new();
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
  let mut layout = Headless::new();
  let a = layout.add("a", Fixed::new(80.0, 10.0));
  let c = layout.add("c", Fixed::new(120.0, 10.0));
  let root = layout.parent("stack", stack(Orientation::Vertical, 0.0), &[a, c]);

  layout.run(root, Size::new(f32::INFINITY, f32::INFINITY), 1.0);

  layout.assert_desired(root, Size::new(120.0, 20.0));
}

// A huge spacing keeps every size and rectangle finite.
#[test]
fn huge_spacing_stays_finite() {
  let mut layout = Headless::new();
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

// A child too tall to fit after the ones before it is cut to end at a finite coordinate, keeping
// nearly all the room left, also where no snapping to the pixel grid would clamp it.
#[test]
fn a_child_too_tall_to_fit_is_cut_to_a_finite_end() {
  let mut layout = Headless::new();
  layout.set_snap(false);
  let a = layout.add("a", Fixed::new(10.0, 1.0e38));
  let c = layout.add("c", Fixed::new(10.0, f32::MAX));
  let root = layout.parent("stack", stack(Orientation::Vertical, 0.0), &[a, c]);

  layout.run(root, WINDOW, 1.0);

  let rect = layout.manager().rect(c).expect("arranged");
  assert!((rect.y + rect.height).is_finite(), "{rect:?}");
  assert!(rect.height > 2.0e38, "{rect:?}");
}

// A stack panel draws its background.
#[test]
fn stack_panel_draws_its_background() {
  let mut layout = Headless::new();
  let panel = stack(Orientation::Vertical, 0.0);
  panel.panel.background.set(Some(Color::rgb(255, 0, 0).into()));
  let child = layout.add("child", Fixed::new(10.0, 10.0));
  let root = layout.parent("stack", panel, &[child]);

  layout.run(root, WINDOW, 1.0);
  let commands = layout.record(root);

  assert!(
    matches!(commands.as_slice(), [Command::Fill { brush, .. }] if *brush == Color::rgb(255, 0, 0).into()),
    "{commands:?}"
  );
}

// The children field is the type's content, taken by the `children` method.
#[test]
fn stack_children_are_its_content() {
  assert!(matches!(
    <StackPanel as UiType>::TYPE_INFO.content,
    Some(ContentInfo::Collection {
      field: "children",
      method: Some(ContentMethod::Children),
      ..
    })
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
    let mut layout = Headless::new();
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

// The builder links a panel's children in order, in both trees.
#[test]
fn children_builder_links_in_order_in_both_trees() {
  let mut tree = UiTree::new();
  let first = StackPanel::new().build(&mut tree);
  let second = StackPanel::new().build(&mut tree);

  let panel = StackPanel::new().spacing(4.0).children([first, second]).build(&mut tree);

  for kind in TreeKind::ALL {
    assert_eq!(tree.children(kind, panel.id()).collect::<Vec<_>>(), [first.id(), second.id()]);
  }
  assert_eq!(panel.get(&tree).spacing.get(), 4.0);
}

// A child that already has a parent moves to the panel being built, as `Children::add` moves it.
#[test]
fn children_builder_moves_a_child_from_its_parent() {
  let mut tree = UiTree::new();
  let child = StackPanel::new().build(&mut tree);
  let old = StackPanel::new().children([child]).build(&mut tree);

  let new = StackPanel::new().children([child]).build(&mut tree);

  assert_eq!(tree.children(TreeKind::Logical, old.id()).count(), 0);
  assert_eq!(tree.parent(TreeKind::Logical, child.id()), Some(new.id()));
}
