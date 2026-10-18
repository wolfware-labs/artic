// Border puts its one child inside its padding and outline, and draws its chrome around it.

use artic_controls::{Border, StackPanel};
use artic_core::tree::{NodeId, UiTree};
use artic_element::{AccessCx, Child, ElementTypes, RenderCx, Visibility};
use artic_layout::LayoutNodes;
use artic_layout::Layoutable;
use artic_layout::geometry::{CornerRadius, Point, Rect, RoundedRect, Size, Thickness};
use artic_registry::{ContentInfo, UiType, ValueError};
use artic_render::{BoxShadow, Brush, Color, Command, Compositor, Layer, Layers, Nodes, SceneBuilder, SceneItem, Shape};
use artic_test::layout::{Boxes, Fixed, Headless, Invariants, strategy};
use proptest::prelude::*;

/// Real elements first, then the shared test leaves.
struct Elements(ElementTypes);

impl LayoutNodes for Elements {
  fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable> {
    self.0.layoutable(tree, node).or_else(|| Boxes.layoutable(tree, node))
  }
}

const WINDOW: Size = Size::new(200.0, 300.0);

fn layout() -> Headless<Elements> {
  Headless::new(Elements(ElementTypes::from_link_time()))
}

fn border(thickness: Thickness, padding: Thickness) -> Border {
  let border = Border::default();
  border.border_thickness.set(thickness);
  border.padding.set(padding);
  border
}

// Border asks for its child's size plus the thickness and padding on each side.
#[test]
fn border_measure_includes_padding_and_thickness() {
  let mut layout = layout();
  let child = layout.add("child", Fixed::new(50.0, 20.0));
  let root = layout.parent(
    "border",
    border(Thickness::new(1.0, 2.0, 3.0, 4.0), Thickness::uniform(10.0)),
    &[child],
  );

  layout.run(root, WINDOW, 1.0);

  layout.assert_desired(root, Size::new(74.0, 46.0));
}

// The child is arranged inside the outline and the padding.
#[test]
fn border_arranges_child_inside_padding() {
  let mut layout = layout();
  let child = layout.add("child", Fixed::new(50.0, 20.0));
  let root = layout.parent(
    "border",
    border(Thickness::new(1.0, 2.0, 3.0, 4.0), Thickness::uniform(10.0)),
    &[child],
  );

  layout.run(root, WINDOW, 1.0);

  layout.assert_rect(child, Rect::new(11.0, 12.0, 176.0, 274.0));
}

// Without a child, Border asks for its thickness and padding.
#[test]
fn border_without_a_child_is_its_chrome() {
  let mut layout = layout();
  let root = layout.add("border", border(Thickness::uniform(2.0), Thickness::uniform(3.0)));

  layout.run(root, WINDOW, 1.0);

  layout.assert_desired(root, Size::new(10.0, 10.0));
}

// Thickness and padding larger than the box leave the child a zero size at an origin inside it.
#[test]
fn thick_border_and_padding_stay_non_negative() {
  let mut layout = layout();
  let child = layout.add("child", Fixed::new(5.0, 5.0));
  let outer = border(Thickness::uniform(30.0), Thickness::uniform(30.0));
  outer.layout.width.set(Some(40.0));
  outer.layout.height.set(Some(40.0));
  let root = layout.parent("border", outer, &[child]);

  layout.run(root, WINDOW, 1.0);

  layout.assert_rect(child, Rect::new(40.0, 40.0, 0.0, 0.0));
}

// With infinite space the child is offered infinity, and Border asks for a finite size.
#[test]
fn border_measure_with_infinite_space() {
  let mut layout = layout();
  let child = layout.add("child", Fixed::new(50.0, 20.0));
  let root = layout.parent("border", border(Thickness::uniform(1.0), Thickness::uniform(2.0)), &[child]);

  layout.run(root, Size::new(f32::INFINITY, f32::INFINITY), 1.0);

  layout.assert_desired(root, Size::new(56.0, 26.0));
}

// Negative, NaN and infinite sides of thickness or padding count as 0.
#[test]
fn border_ignores_invalid_sides() {
  for side in [-5.0, f32::NAN, f32::INFINITY] {
    let mut layout = layout();
    let child = layout.add("child", Fixed::new(50.0, 20.0));
    let root = layout.parent("border", border(Thickness::uniform(side), Thickness::uniform(side)), &[child]);

    layout.run(root, WINDOW, 1.0);

    layout.assert_desired(root, Size::new(50.0, 20.0));
    layout.assert_rect(child, Rect::new(0.0, 0.0, 200.0, 300.0));
  }
}

// A collapsed child takes no part: Border asks for its chrome only.
#[test]
fn collapsed_child_leaves_the_chrome() {
  let mut layout = layout();
  let hidden = StackPanel::default();
  hidden.appearance.visibility.set(Visibility::Collapsed);
  let child = layout.add("child", hidden);
  let root = layout.parent("border", border(Thickness::uniform(2.0), Thickness::uniform(3.0)), &[child]);

  layout.run(root, WINDOW, 1.0);

  layout.assert_desired(root, Size::new(10.0, 10.0));
}

// The child field is the type's XAML content.
#[test]
fn border_child_is_its_content() {
  assert!(matches!(
    <Border as UiType>::TYPE_INFO.content,
    Some(ContentInfo::Collection { field: "child", .. })
  ));
}

// Content takes one child; a second one is refused.
#[test]
fn border_takes_one_child() {
  let Some(ContentInfo::Collection { add_child, .. }) = <Border as UiType>::TYPE_INFO.content else {
    panic!("collection content");
  };
  let mut tree = UiTree::new();
  let border = tree.alloc(Box::new(Border::default()));
  let first = tree.alloc(Box::new(StackPanel::default()));
  let second = tree.alloc(Box::new(StackPanel::default()));

  assert_eq!(add_child(&mut tree, border, first), Ok(()));
  assert!(matches!(add_child(&mut tree, border, second), Err(ValueError::OneChildOnly)));
}

// Border adds nothing to accessibility: a generic container.
#[test]
fn border_accesskit_node() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Border::default()));
  let types = ElementTypes::from_link_time();
  let element = types.element(tree.get(node).expect("a node")).expect("an element");

  let access = element.accessibility(&AccessCx::new(&tree, node, Rect::new(0.0, 0.0, 10.0, 10.0)));

  assert_eq!(access.role(), accesskit::Role::GenericContainer);
}

const SIZE: Size = Size::new(40.0, 30.0);

/// Composes one Border root of `SIZE`, with an optional child of the same size.
struct OneSize(ElementTypes);

impl Nodes for OneSize {
  fn layers(&self, tree: &UiTree, node: NodeId) -> Layers {
    tree
      .get(node)
      .and_then(|object| self.0.element(object))
      .map_or_else(Layers::default, |element| element.layers(SIZE))
  }

  fn record(&mut self, tree: &UiTree, node: NodeId, builder: &mut SceneBuilder) {
    if let Some(element) = tree.get(node).and_then(|object| self.0.element(object)) {
      element.render(&mut RenderCx::new(tree, node, SIZE, None), builder);
    }
  }
}

fn compose(border: Border, child: bool) -> Vec<SceneItem> {
  let mut tree = UiTree::new();
  let root = tree.alloc(Box::new(border));
  if child {
    let leaf = tree.alloc(Box::new(StackPanel::default()));
    Child::set(&mut tree, root, Some(leaf));
  }
  Compositor::default()
    .compose(&mut tree, root, &mut OneSize(ElementTypes::from_link_time()))
    .items()
    .to_vec()
}

fn commands(border: Border) -> Vec<Command> {
  compose(border, false)
    .into_iter()
    .find_map(|item| match item {
      SceneItem::Fragment(fragment) => Some(fragment.commands().to_vec()),
      _ => None,
    })
    .unwrap_or_default()
}

fn chrome() -> Border {
  let border = Border::default();
  border.border_thickness.set(Thickness::new(1.0, 2.0, 3.0, 4.0));
  border.corner_radius.set(CornerRadius::uniform(6.0));
  border
}

fn outline() -> RoundedRect {
  RoundedRect::new(Rect::new(0.0, 0.0, SIZE.width, SIZE.height), CornerRadius::uniform(6.0))
    .fitted_for_border(Thickness::new(1.0, 2.0, 3.0, 4.0))
}

// The shadow, the background and the ring are drawn in that order, all from one outline.
#[test]
fn border_draws_shadow_background_then_ring() {
  let border = chrome();
  let shadow = BoxShadow::new(Point::new(0.0, 2.0), 4.0, Color::rgba(0, 0, 0, 64));
  border.box_shadow.set(Some(shadow.clone()));
  border.background.set(Some(Color::rgb(255, 255, 255).into()));
  border.border_brush.set(Some(Color::rgb(0, 0, 0).into()));

  assert_eq!(
    commands(border),
    [
      Command::BoxShadow { rect: outline(), shadow },
      Command::Fill {
        shape: Shape::Path(outline().path()),
        brush: Brush::from(Color::rgb(255, 255, 255))
      },
      Command::Fill {
        shape: Shape::Path(outline().ring(Thickness::new(1.0, 2.0, 3.0, 4.0))),
        brush: Brush::from(Color::rgb(0, 0, 0))
      },
    ]
  );
}

// A brush with every side zero, or sides with no brush, draws no ring.
#[test]
fn no_ring_without_brush_or_width() {
  let zero = Border::default();
  zero.border_brush.set(Some(Color::rgb(0, 0, 0).into()));

  assert_eq!(commands(zero), []);
  assert_eq!(commands(chrome()), []);
}

// Sides that are all invalid count as zero, so a brush alone draws no ring.
#[test]
fn invalid_thickness_draws_no_ring() {
  let border = Border::default();
  border
    .border_thickness
    .set(Thickness::new(-1.0, f32::NAN, f32::INFINITY, -0.5));
  border.border_brush.set(Some(Color::rgb(0, 0, 0).into()));

  assert_eq!(commands(border), []);
}

// The child and its subtree are clipped to the inside of the outline.
#[test]
fn border_clips_to_the_inner_shape() {
  let items = compose(chrome(), true);

  assert!(items.contains(&SceneItem::Push(Layer::Clip(Shape::Path(
    outline().inner(Thickness::new(1.0, 2.0, 3.0, 4.0))
  )))));
}

// Without a drawn child, the compositor pushes no clip.
#[test]
fn no_clip_without_a_drawn_child() {
  let items = compose(chrome(), false);

  assert!(!items.iter().any(|item| matches!(item, SceneItem::Push(Layer::Clip(_)))));
}

proptest! {
  // Any thickness and padding, available size and scale: the answer is the same from a fresh
  // manager, and the child stays inside the border.
  #[test]
  fn border_invariants_hold(
    sides in proptest::array::uniform8(0.0..80.0f32),
    child in (0.0..300.0f32, 0.0..300.0f32),
    available in strategy::available(),
    scale in strategy::scale(),
  ) {
    let mut layout = layout();
    let leaf = layout.add("child", Fixed::new(child.0, child.1));
    let [l, t, r, b, pl, pt, pr, pb] = sides;
    let root = layout.parent("border", border(Thickness::new(l, t, r, b), Thickness::new(pl, pt, pr, pb)), &[leaf]);

    Invariants::default().contained().assert(&mut layout, root, available, scale);
  }
}
