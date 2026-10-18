// The property groups: their properties, registered through a control that flattens them, and
// their shared work: layout, hit testing, input, accessibility and layers.

use artic_core::Object;
use artic_core::property::{Affects, AnyProperty};
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_element::{AccessCx, Appearance, Classes, HitTestCx, HitTestResult, Identity, Interaction, Layout, Visibility};
use artic_layout::geometry::{Point, Rect, Size, Thickness, Transform};
use artic_layout::{
  ArrangeCx, HorizontalAlignment, LayoutManager, LayoutNodes, Layoutable, MeasureCx, PixelGrid, VerticalAlignment,
};
use artic_macros::UiType;
use artic_registry::{PropertyGroup, Registry};
use artic_render::{Layers, Shape};
use artic_text::Engine;
use pretty_assertions::assert_eq;
use std::cell::Cell;

/// A control with the four groups and nothing else.
#[derive(Debug, UiType)]
struct Probe {
  #[ui(flatten)]
  layout: Layout,
  #[ui(flatten)]
  appearance: Appearance,
  #[ui(flatten)]
  interaction: Interaction,
  #[ui(flatten)]
  identity: Identity,
}

/// A second control type, to check inheritance across types.
#[derive(Debug, UiType)]
struct Other {
  #[ui(flatten)]
  interaction: Interaction,
}

/// The markup name of each property, and the field of the group that holds it.
const NAMES: [(&str, &str); 20] = [
  ("Width", "layout"),
  ("Height", "layout"),
  ("MinWidth", "layout"),
  ("MinHeight", "layout"),
  ("MaxWidth", "layout"),
  ("MaxHeight", "layout"),
  ("Margin", "layout"),
  ("HorizontalAlignment", "layout"),
  ("VerticalAlignment", "layout"),
  ("Visibility", "appearance"),
  ("Opacity", "appearance"),
  ("RenderTransform", "appearance"),
  ("ZIndex", "appearance"),
  ("IsEnabled", "interaction"),
  ("IsHitTestVisible", "interaction"),
  ("Focusable", "interaction"),
  ("Cursor", "interaction"),
  ("ToolTip", "interaction"),
  ("Name", "identity"),
  ("Classes", "identity"),
];

// Every property is found on a control that flattens the groups, under its group's field.
#[test]
fn groups_registered() {
  let registry = Registry::from_link_time().expect("no duplicates");
  let info = registry
    .type_by_name("properties", "Probe")
    .found()
    .expect("Probe is registered");

  for (name, group) in NAMES {
    let found = info.property(name).unwrap_or_else(|| panic!("{name} is a property of Probe"));
    assert_eq!(found.group, Some(group), "{name}");
  }
  assert_eq!(info.property("Margin").unwrap().info.affects, Affects::LAYOUT);
  assert!(info.property("IsEnabled").unwrap().info.inherits);
  let count: usize = info.groups.iter().map(|group| group.properties.len()).sum();
  assert_eq!(count, NAMES.len());
}

// Defaults: no explicit size, unbounded maximum, enabled, opaque, hit-testable, no classes.
#[test]
fn group_defaults() {
  let layout = Layout::default();
  let interaction = Interaction::default();

  assert_eq!((layout.width.get(), layout.max_width.get()), (None, f32::INFINITY));
  assert!(interaction.is_enabled.get() && interaction.is_hit_test_visible.get());
  assert_eq!(Appearance::default().opacity.get(), 1.0);
  assert_eq!(Identity::default().classes.get(), Classes::default());
}

// Classes parse from space-separated names.
#[test]
fn classes_parse_from_names() {
  let classes: Classes = "primary  large".parse().unwrap();

  assert!(classes.contains("large") && !classes.contains("small"));
}

/// An element with the layout and appearance groups and a fixed content size, which measures its
/// children and counts its own measures.
#[derive(Debug)]
struct Boxed {
  layout: Layout,
  appearance: Appearance,
  content: Size,
  measured: Cell<u32>,
}

impl Boxed {
  /// A box whose content is `width` × `height`.
  fn new(width: f32, height: f32) -> Self {
    Self {
      layout: Layout::default(),
      appearance: Appearance::default(),
      content: Size::new(width, height),
      measured: Cell::new(0),
    }
  }
}

impl Object for Boxed {
  fn visit_properties(&self, visit: &mut dyn FnMut(&dyn AnyProperty)) {
    self.layout.visit_properties(visit);
    self.appearance.visit_properties(visit);
  }
}

impl Layoutable for Boxed {
  fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
    self.measured.set(self.measured.get() + 1);
    self.layout.measure(cx, available, |cx, inner| {
      let children: Vec<NodeId> = cx.children().collect();
      for child in children {
        cx.measure(child, inner);
      }
      self.content
    })
  }

  fn arrange(&self, cx: &mut ArrangeCx<'_>, slot: Rect) {
    self.layout.arrange(cx, slot, |cx, size| {
      let children: Vec<NodeId> = cx.children().collect();
      for child in children {
        cx.arrange(child, Rect::new(0.0, 0.0, size.width, size.height));
      }
    });
  }

  fn collapsed(&self) -> bool {
    self.appearance.visibility.get() == Visibility::Collapsed
  }
}

/// Lays out `Boxed` nodes.
struct Boxes;

impl LayoutNodes for Boxes {
  fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable> {
    tree.get_as::<Boxed>(node).map(|boxed| boxed as &dyn Layoutable)
  }
}

/// Lays out one box in a 200 × 100 slot and returns its bounds.
fn bounds_of(boxed: Boxed) -> Rect {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(boxed));
  let mut layout = LayoutManager::default();
  layout.run(
    &mut tree,
    &Boxes,
    node,
    Size::new(200.0, 100.0),
    PixelGrid { scale: 1.0, snap: false },
    &mut Engine::with_fonts([]),
  );
  layout.rect(node).expect("arranged")
}

// With a 10-unit margin, each alignment places a 40 × 20 content against its edge, in the middle,
// or across the slot inside the margin.
#[test]
fn margin_and_alignment_layout() {
  use HorizontalAlignment as H;
  use VerticalAlignment as V;
  let at = |horizontal, vertical| {
    let boxed = Boxed::new(40.0, 20.0);
    boxed.layout.margin.set(Thickness::uniform(10.0));
    boxed.layout.horizontal_alignment.set(horizontal);
    boxed.layout.vertical_alignment.set(vertical);
    bounds_of(boxed)
  };

  assert_eq!(at(H::Left, V::Top), Rect::new(10.0, 10.0, 40.0, 20.0));
  assert_eq!(at(H::Center, V::Center), Rect::new(80.0, 40.0, 40.0, 20.0));
  assert_eq!(at(H::Right, V::Bottom), Rect::new(150.0, 70.0, 40.0, 20.0));
  assert_eq!(at(H::Stretch, V::Stretch), Rect::new(10.0, 10.0, 180.0, 80.0));
}

// The minimum wins over the content and the maximum caps it, in the desired size.
#[test]
fn min_max_clamp_desired_size() {
  let mut tree = UiTree::new();
  let boxed = Boxed::new(10.0, 50.0);
  boxed.layout.min_width.set(60.0);
  boxed.layout.max_height.set(30.0);
  let node = tree.alloc(Box::new(boxed));
  let mut layout = LayoutManager::default();

  layout.run(
    &mut tree,
    &Boxes,
    node,
    Size::new(200.0, 100.0),
    PixelGrid { scale: 1.0, snap: false },
    &mut Engine::with_fonts([]),
  );

  assert_eq!(layout.desired(node), Some(Size::new(60.0, 30.0)));
}

// A collapsed element has a zero desired size and bounds, its child isn't laid out, and a child
// made dirty afterwards isn't measured.
#[test]
fn collapsed_takes_no_space() {
  let mut tree = UiTree::new();
  let parent = tree.alloc(Box::new(Boxed::new(40.0, 20.0)));
  let child = tree.alloc(Box::new(Boxed::new(10.0, 10.0)));
  tree.add_child(TreeKind::Visual, parent, child);
  let mut layout = LayoutManager::default();
  let available = Size::new(200.0, 100.0);
  layout.run(
    &mut tree,
    &Boxes,
    parent,
    available,
    PixelGrid { scale: 1.0, snap: false },
    &mut Engine::with_fonts([]),
  );

  tree
    .get_as::<Boxed>(parent)
    .unwrap()
    .appearance
    .visibility
    .set(Visibility::Collapsed);
  layout.run(
    &mut tree,
    &Boxes,
    parent,
    available,
    PixelGrid { scale: 1.0, snap: false },
    &mut Engine::with_fonts([]),
  );
  assert_eq!((layout.desired(parent), layout.rect(parent)), (None, None));
  assert_eq!((layout.desired(child), layout.rect(child)), (None, None));

  LayoutManager::invalidate_measure(&mut tree, child);
  layout.run(
    &mut tree,
    &Boxes,
    parent,
    available,
    PixelGrid { scale: 1.0, snap: false },
    &mut Engine::with_fonts([]),
  );
  assert_eq!(tree.get_as::<Boxed>(child).unwrap().measured.get(), 1);
}

// IsEnabled inherits across control types, since every control flattens the interaction group's
// one key.
#[test]
fn is_enabled_inherits_across_control_types() {
  let mut tree = UiTree::new();
  let probe = tree.alloc(Box::new(Probe::default()));
  let other = tree.alloc(Box::new(Other::default()));
  tree.add_child(TreeKind::Logical, probe, other);

  tree.get_as::<Probe>(probe).unwrap().interaction.is_enabled.set(false);

  assert!(!tree.get_as::<Other>(other).unwrap().interaction.is_enabled.get());
}

// With snapping, the content is arranged in the element's snapped box, so a stretched child is
// exactly as wide as its parent's recorded bounds.
#[test]
fn content_is_arranged_in_the_snapped_bounds() {
  let mut tree = UiTree::new();
  let parent = Boxed::new(40.8, 10.0);
  parent.layout.horizontal_alignment.set(HorizontalAlignment::Center);
  let parent = tree.alloc(Box::new(parent));
  let child = tree.alloc(Box::new(Boxed::new(0.0, 0.0)));
  tree.add_child(TreeKind::Visual, parent, child);
  let mut layout = LayoutManager::default();

  layout.run(
    &mut tree,
    &Boxes,
    parent,
    Size::new(100.0, 10.0),
    PixelGrid { scale: 1.0, snap: true },
    &mut Engine::with_fonts([]),
  );

  let width = |node| layout.rect(node).expect("arranged").width;
  assert_eq!(width(child), width(parent));
}

// A visible, hit-testable element is hit inside its bounds; hidden, collapsed and click-through
// elements are missed even there.
#[test]
fn hit_test_misses_hidden_and_click_through() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Probe::default()));
  let cx = HitTestCx::new(&tree, node, Size::new(10.0, 10.0));
  let inside = Point::new(5.0, 5.0);
  let interaction = Interaction::default();

  assert_eq!(interaction.hit_test(&cx, Visibility::Visible, inside), HitTestResult::Hit);
  assert_eq!(
    interaction.hit_test(&cx, Visibility::Visible, Point::new(15.0, 5.0)),
    HitTestResult::Miss
  );
  assert_eq!(interaction.hit_test(&cx, Visibility::Hidden, inside), HitTestResult::Miss);
  assert_eq!(interaction.hit_test(&cx, Visibility::Collapsed, inside), HitTestResult::Miss);
  interaction.is_hit_test_visible.set(false);
  assert_eq!(interaction.hit_test(&cx, Visibility::Visible, inside), HitTestResult::Miss);
}

// An element accepts input until it's disabled, locally or by a logical ancestor.
#[test]
fn accepts_input_until_disabled() {
  let mut tree = UiTree::new();
  let parent = tree.alloc(Box::new(Probe::default()));
  let child = tree.alloc(Box::new(Other::default()));
  tree.add_child(TreeKind::Logical, parent, child);
  let accepts = |tree: &UiTree| tree.get_as::<Other>(child).unwrap().interaction.accepts_input();

  assert!(accepts(&tree));
  tree.get_as::<Probe>(parent).unwrap().interaction.is_enabled.set(false);
  assert!(!accepts(&tree));
}

// The default accessibility node is a generic container at the element's bounds.
#[test]
fn accessibility_node_has_bounds() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Probe::default()));
  let cx = AccessCx::new(&tree, node, Rect::new(10.0, 20.0, 30.0, 40.0));

  let built = Interaction::default().accessibility(&cx);

  assert_eq!(built.role(), accesskit::Role::GenericContainer);
  assert_eq!(built.bounds(), Some(accesskit::Rect::new(10.0, 20.0, 40.0, 60.0)));
}

// The layers carry the control's clip, and otherwise the defaults.
#[test]
fn layers_carry_the_clip() {
  let clip = Some(Shape::Rect(Rect::new(0.0, 0.0, 5.0, 5.0)));

  let layers = Appearance::default().layers(Size::new(10.0, 10.0), clip.clone());

  assert_eq!(
    layers,
    Layers {
      clip,
      ..Layers::default()
    }
  );
}

// Opacity outside 0–1 is clamped, and NaN counts as opaque; the property keeps what was set.
#[test]
fn layers_clamp_opacity() {
  let appearance = Appearance::default();
  let opacity = |value: f32| {
    appearance.opacity.set(value);
    appearance.layers(Size::new(10.0, 10.0), None).opacity
  };

  assert_eq!(
    (opacity(0.25), opacity(-1.0), opacity(2.0), opacity(f32::NAN)),
    (0.25, 0.0, 1.0, 1.0)
  );
  assert!(appearance.opacity.get().is_nan());
}

// A rotation pivots on the element's centre, which stays where it was.
#[test]
fn transform_pivots_on_the_centre() {
  let appearance = Appearance::default();
  appearance
    .render_transform
    .set(Some(Transform::rotate(std::f32::consts::FRAC_PI_2)));

  let transform = appearance
    .layers(Size::new(100.0, 40.0), None)
    .transform
    .expect("a transform");

  let centre = transform.apply(Point::new(50.0, 20.0));
  assert!((centre.x - 50.0).abs() < 1e-4 && (centre.y - 20.0).abs() < 1e-4, "{centre:?}");
}

// A scale grows the element away from its centre, so its top-left corner moves up and left.
#[test]
fn scale_grows_from_the_centre() {
  let appearance = Appearance::default();
  appearance.render_transform.set(Some(Transform::scale(2.0, 2.0)));

  let transform = appearance
    .layers(Size::new(100.0, 40.0), None)
    .transform
    .expect("a transform");

  assert_eq!(transform.apply(Point::new(0.0, 0.0)), Point::new(-50.0, -20.0));
}

// Hidden and collapsed elements aren't visible in composition, and ZIndex reaches the layers.
#[test]
fn layers_carry_visibility_and_z_index() {
  let appearance = Appearance::default();
  appearance.z_index.set(3);
  let visible = |visibility| {
    appearance.visibility.set(visibility);
    appearance.layers(Size::new(1.0, 1.0), None).visible
  };

  assert_eq!(
    (
      visible(Visibility::Visible),
      visible(Visibility::Hidden),
      visible(Visibility::Collapsed)
    ),
    (true, false, false)
  );
  assert_eq!(appearance.layers(Size::new(1.0, 1.0), None).z_index, 3);
}

// A disabled element's accessibility node says so.
#[test]
fn accessibility_node_is_disabled_with_the_element() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Probe::default()));
  let cx = AccessCx::new(&tree, node, Rect::new(0.0, 0.0, 1.0, 1.0));
  let interaction = Interaction::default();

  assert!(!interaction.accessibility(&cx).is_disabled());
  interaction.is_enabled.set(false);
  assert!(interaction.accessibility(&cx).is_disabled());
}

// An element under a disabled ancestor gets a disabled node, even with its own IsEnabled true.
#[test]
fn accessibility_node_is_disabled_under_a_disabled_ancestor() {
  let mut tree = UiTree::new();
  let parent = tree.alloc(Box::new(Probe::default()));
  let child = tree.alloc(Box::new(Probe::default()));
  tree.add_child(TreeKind::Logical, parent, child);
  tree.get_as::<Probe>(child).unwrap().interaction.is_enabled.set(true);
  tree.get_as::<Probe>(parent).unwrap().interaction.is_enabled.set(false);
  let cx = AccessCx::new(&tree, child, Rect::new(0.0, 0.0, 1.0, 1.0));

  assert!(
    tree
      .get_as::<Probe>(child)
      .unwrap()
      .interaction
      .accessibility(&cx)
      .is_disabled()
  );
}
