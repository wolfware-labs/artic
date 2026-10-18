// #[element]: a control gets the property groups, its ElementImpl hooks run behind their shared
// work, its defaults apply when it's created, and it reaches the cast table without a
// registration call.

use artic_core::input::InputEvent;
use artic_core::property::{Precedence, Property};
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_element as element_crate;
use artic_element::{
  AccessCx, Cursor, Element, ElementImpl, ElementTypes, EventCx, Handled, HasState, HitTestCx, HitTestResult, OverrideDefaults,
  RenderCx, Visibility,
};
use artic_layout::geometry::{Point, Rect, Size, Thickness};
use artic_layout::{ArrangeCx, LayoutManager, MeasureCx, PixelGrid};
use artic_macros::element;
use artic_registry::XamlType;
use artic_render::{SceneBuilder, Shape};
use artic_text::Engine;
use pretty_assertions::assert_eq;
use std::cell::Cell;

/// A control with fixed content that counts its measures, draws and input, records the size it
/// arranges its content in, and adds a label and a clip.
#[element]
#[derive(Debug)]
#[xaml(namespace = "https://artic.rs/test")]
struct Tile {
  measured: Cell<u32>,
  arranged: Cell<Option<Size>>,
  rendered: Cell<u32>,
  events: u32,
}

impl ElementImpl for Tile {
  fn measure_override(&self, _cx: &mut MeasureCx<'_>, _available: Size) -> Size {
    self.measured.set(self.measured.get() + 1);
    Size::new(30.0, 10.0)
  }

  fn arrange_override(&self, _cx: &mut ArrangeCx<'_>, size: Size) {
    self.arranged.set(Some(size));
  }

  fn render(&self, _cx: &mut RenderCx<'_>, _scene: &mut SceneBuilder) {
    self.rendered.set(self.rendered.get() + 1);
  }

  fn on_event(&mut self, _cx: &mut EventCx<'_>, _event: &InputEvent) -> Handled {
    self.events += 1;
    Handled::Yes
  }

  fn accessibility(&self, _cx: &AccessCx<'_>, node: &mut accesskit::Node) {
    node.set_label("tile");
  }

  fn clip(&self, size: Size) -> Option<Shape> {
    Some(Shape::Rect(Rect::new(0.0, 0.0, size.width, size.height)))
  }
}

/// Sends one default input event to `node` through the cast table.
fn send(tree: &mut UiTree, node: NodeId) -> Handled {
  let types = ElementTypes::from_link_time();
  let element = types.element_mut(tree.get_mut(node).unwrap()).expect("Tile is registered");
  element.on_event(&mut EventCx::new(node), &InputEvent::default())
}

// The Tile is found through the cast table and laid out with its margin around its
// measure_override, and arrange_override gets the size inside the margin.
#[test]
fn element_lays_out_through_cast_table() {
  let mut tree = UiTree::new();
  let tile = Tile::default();
  tile.layout.margin.set(Thickness::uniform(5.0));
  let node = tree.alloc(Box::new(tile));
  let mut layout = LayoutManager::default();

  layout.run(
    &mut tree,
    &ElementTypes::from_link_time(),
    node,
    Size::new(100.0, 100.0),
    PixelGrid { scale: 1.0, snap: false },
    &mut Engine::with_fonts([]),
  );

  assert_eq!(layout.desired(node), Some(Size::new(40.0, 20.0)));
  assert_eq!(layout.rect(node), Some(Rect::new(5.0, 5.0, 90.0, 90.0)));
  assert_eq!(tree.get_as::<Tile>(node).unwrap().arranged.get(), Some(Size::new(90.0, 90.0)));
}

// Element::render calls the control's hook once.
#[test]
fn element_render_calls_hook() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Tile::default()));
  let tile = tree.get_as::<Tile>(node).unwrap();

  Element::render(
    tile,
    &mut RenderCx::new(&tree, node, Size::ZERO),
    &mut SceneBuilder::default(),
  );

  assert_eq!(tile.rendered.get(), 1);
}

// A collapsed Tile takes no part in layout: its measure_override isn't called and it keeps no size.
#[test]
fn collapsed_element_skips_measure_override() {
  let mut tree = UiTree::new();
  let tile = Tile::default();
  tile.appearance.visibility.set(Visibility::Collapsed);
  let node = tree.alloc(Box::new(tile));
  let mut layout = LayoutManager::default();

  layout.run(
    &mut tree,
    &ElementTypes::from_link_time(),
    node,
    Size::new(100.0, 100.0),
    PixelGrid { scale: 1.0, snap: false },
    &mut Engine::with_fonts([]),
  );

  assert_eq!(layout.desired(node), None);
  assert_eq!(tree.get_as::<Tile>(node).unwrap().measured.get(), 0);
}

// An enabled Tile's hook runs exactly once per event; a disabled one's doesn't run.
#[test]
fn disabled_element_hook_not_called() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Tile::default()));

  assert_eq!(send(&mut tree, node), Handled::Yes);
  assert_eq!(tree.get_as::<Tile>(node).unwrap().events, 1);
  tree.get_as::<Tile>(node).unwrap().interaction.is_enabled.set(false);
  assert_eq!(send(&mut tree, node), Handled::No);
  assert_eq!(tree.get_as::<Tile>(node).unwrap().events, 1);
}

// A Tile inside a disabled Tile gets no input either, since IsEnabled inherits.
#[test]
fn inherited_disabled_skips_hook() {
  let mut tree = UiTree::new();
  let parent = tree.alloc(Box::new(Tile::default()));
  let child = tree.alloc(Box::new(Tile::default()));
  tree.add_child(TreeKind::Logical, parent, child);
  tree.get_as::<Tile>(parent).unwrap().interaction.is_enabled.set(false);

  assert_eq!(send(&mut tree, child), Handled::No);
  assert_eq!(tree.get_as::<Tile>(child).unwrap().events, 0);
}

// A disabled parent disables a child whose own IsEnabled is true, so its hook doesn't run.
#[test]
fn disabled_parent_overrides_child_enabled() {
  let mut tree = UiTree::new();
  let parent = tree.alloc(Box::new(Tile::default()));
  let child = tree.alloc(Box::new(Tile::default()));
  tree.add_child(TreeKind::Logical, parent, child);
  tree.get_as::<Tile>(child).unwrap().interaction.is_enabled.set(true);
  tree.get_as::<Tile>(parent).unwrap().interaction.is_enabled.set(false);

  assert_eq!(send(&mut tree, child), Handled::No);
  assert_eq!(tree.get_as::<Tile>(child).unwrap().events, 0);
}

// Element::hit_test is the interaction group's: a hidden Tile is missed inside its bounds.
#[test]
fn element_hit_test_misses_hidden_element() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Tile::default()));
  let cx = HitTestCx::new(&tree, node, Size::new(10.0, 10.0));
  let tile = tree.get_as::<Tile>(node).unwrap();

  assert_eq!(Element::hit_test(tile, &cx, Point::new(5.0, 5.0)), HitTestResult::Hit);
  tile.appearance.visibility.set(Visibility::Hidden);
  assert_eq!(Element::hit_test(tile, &cx, Point::new(5.0, 5.0)), HitTestResult::Miss);
}

// The control's accessibility hook adds to the default node, and its clip reaches the layers.
#[test]
fn control_hooks_add_to_default_node_and_layers() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Tile::default()));
  let tile = tree.get_as::<Tile>(node).unwrap();
  let cx = AccessCx::new(&tree, node, Rect::new(0.0, 0.0, 8.0, 4.0));

  let built = Element::accessibility(tile, &cx);
  let layers = Element::layers(tile, Size::new(8.0, 4.0));

  assert_eq!(
    (built.role(), built.label()),
    (accesskit::Role::GenericContainer, Some("tile"))
  );
  assert_eq!(layers.clip, Some(Shape::Rect(Rect::new(0.0, 0.0, 8.0, 4.0))));
}

/// A test-only state: how often a control was pressed.
#[derive(Debug, Default)]
struct Presses {
  count: u32,
}

/// A second test-only state: the control's caption.
#[derive(Debug, Default)]
struct Caption {
  text: String,
}

/// What a control holding `Presses` can do, given to every such control by the blanket impl.
trait Pressable: Element + HasState<Presses> {
  /// How often it was pressed.
  fn presses(&self) -> u32 {
    HasState::<Presses>::state(self).count
  }
}

impl<T: Element + HasState<Presses>> Pressable for T {}

/// What a control holding `Caption` can do.
trait Captioned: Element + HasState<Caption> {
  /// Its caption.
  fn caption(&self) -> &str {
    &HasState::<Caption>::state(self).text
  }
}

impl<T: Element + HasState<Caption>> Captioned for T {}

/// A control with two states.
#[element]
#[derive(Debug)]
#[xaml(namespace = "https://artic.rs/test")]
struct Key {
  #[element(state)]
  presses: Presses,
  #[element(state)]
  caption: Caption,
}

impl ElementImpl for Key {}

// Each state field gives its trait through the blanket impl, and state_mut reaches the field.
#[test]
fn state_fields_give_their_traits() {
  let mut key = Key::default();
  HasState::<Presses>::state_mut(&mut key).count = 2;
  HasState::<Caption>::state_mut(&mut key).text = "Enter".into();

  assert_eq!((key.presses(), key.caption()), (2, "Enter"));
}

/// A control that is focusable and shows a hand by default.
#[element]
#[derive(Debug)]
#[xaml(namespace = "https://artic.rs/test", override_defaults)]
struct Knob {}

impl ElementImpl for Knob {}

impl OverrideDefaults for Knob {
  fn override_defaults(&self) {
    self.interaction.focusable.set_default(true);
    self.interaction.cursor.set_default(Cursor::Hand);
  }
}

// A control's overridden group defaults hold whether it is built by Default or by its TypeInfo's
// constructor.
#[test]
fn knob_overrides_group_defaults() {
  let knob = Knob::default();
  assert!(knob.interaction.focusable.get());
  assert_eq!(knob.interaction.cursor.get(), Cursor::Hand);

  let mut tree = UiTree::new();
  let node = (<Knob as XamlType>::TYPE_INFO.new)(&mut tree);
  assert!(tree.get_as::<Knob>(node).unwrap().interaction.focusable.get());
}

// A style setter wins over a control's default.
#[test]
fn style_setter_beats_control_default() {
  let knob = Knob::default();

  knob.interaction.focusable.set_at(Precedence::StyleSetter, false);

  assert!(!knob.interaction.focusable.get());
}

// `#[element]` adds `derive(XamlType)`, so the control is registered and built like any XAML type.
#[test]
fn element_adds_derive_xaml_type() {
  let mut tree = UiTree::new();

  let node = (<Knob as XamlType>::TYPE_INFO.new)(&mut tree);

  assert!(tree.get_as::<Knob>(node).is_some());
}

/// A control that names its crate path itself.
#[element]
#[derive(Debug)]
#[xaml(namespace = "https://artic.rs/test", crate = "element_crate")]
struct Pinned {}

impl ElementImpl for Pinned {}

// A control that sets `crate` itself keeps it: `#[element]` adds the path only when it's missing.
#[test]
fn element_keeps_its_own_crate_path() {
  assert_eq!(<Pinned as XamlType>::TYPE_INFO.name, "Pinned");
}

/// A control with nothing of its own.
#[element]
#[derive(Debug)]
#[xaml(namespace = "https://artic.rs/test")]
struct Spacer;

impl ElementImpl for Spacer {}

// A unit struct gets the four groups, registered first and in order.
#[test]
fn unit_struct_gets_the_groups() {
  let fields: Vec<&str> = <Spacer as XamlType>::TYPE_INFO
    .groups
    .iter()
    .map(|group| group.field)
    .collect();

  assert_eq!(fields, ["layout", "appearance", "interaction", "identity"]);
}

/// A control with a property of its own, with its own default.
#[element]
#[derive(Debug)]
#[xaml(namespace = "https://artic.rs/test")]
struct Dial {
  #[xaml(default = 3)]
  turns: Property<i32>,
}

impl ElementImpl for Dial {}

// A control's own property is registered beside the groups' and built with its default.
#[test]
fn own_properties_register_beside_the_groups() {
  let info = <Dial as XamlType>::TYPE_INFO;

  assert_eq!(info.property("Turns").map(|found| found.group), Some(None));
  assert_eq!(info.property("Margin").map(|found| found.group), Some(Some("layout")));
  assert_eq!(Dial::default().turns.get(), 3);
}

/// A module where a constant named `value` is in scope, as a user's item might be.
mod shadowed {
  use super::{ElementImpl, element};

  #[allow(non_upper_case_globals, dead_code)]
  const value: u8 = 0;

  /// A control declared beside that constant.
  #[element]
  #[derive(Debug)]
  #[xaml(namespace = "https://artic.rs/test")]
  pub struct Lamp;

  impl ElementImpl for Lamp {}
}

// The generated Default doesn't collide with an item named `value` in the control's module.
#[test]
fn default_ignores_a_user_item_named_value() {
  assert!(shadowed::Lamp::default().interaction.is_enabled.get());
}

// A render context carries the size layout gave the node it draws.
#[test]
fn render_cx_carries_its_size() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Tile::default()));

  let cx = RenderCx::new(&tree, node, Size::new(40.0, 30.0));

  assert_eq!(cx.size(), Size::new(40.0, 30.0));
}
