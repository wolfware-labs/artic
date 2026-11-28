// A UI language's runtime loader writes the controls' properties through the registry's
// property access, knowing only names and text: one value of each kind the controls hold.

use artic_controls::{Border, TextBlock};
use artic_core::property::Precedence;
use artic_core::tree::{NodeId, UiTree};
use artic_layout::HorizontalAlignment;
use artic_layout::geometry::{CornerRadius, Thickness};
use artic_layout::text::TextAlignment;
use artic_registry::{ContentInfo, PropertyAccess, Registry, TypeInfo, UiType};
use artic_render::{Brush, Color, GradientStop, LinearGradientBrush, SolidColorBrush};
use pretty_assertions::assert_eq;

/// Sets `property` of `node`, whose type `type_info` describes, from `text` as the loader does:
/// looked up by name, with no value type in sight.
fn set_text(tree: &mut UiTree, type_info: &'static TypeInfo, node: NodeId, property: &str, text: &str) {
  (access(type_info, property).set_text)(tree, node, text, Precedence::Local).expect("the text parses");
}

/// A control created from its descriptor, as the loader creates one.
fn create(tree: &mut UiTree, type_info: &'static TypeInfo) -> NodeId {
  (type_info.new.expect("the control has a constructor"))(tree)
}

// The registry the loader reads finds the controls these writes go through.
#[test]
fn the_controls_are_registered() {
  let registry = Registry::from_link_time().expect("no duplicates");

  for name in ["Border", "TextBlock"] {
    assert!(registry.type_by_name("artic_controls", name).found().is_some(), "{name}");
  }
}

// A `Border` takes a `Thickness`, a `CornerRadius` and a brush, each from text.
#[test]
fn border_values_parse_from_text() {
  let mut tree = UiTree::new();
  let info = Border::TYPE_INFO;
  let border = create(&mut tree, info);

  set_text(&mut tree, info, border, "Padding", "4,8");
  set_text(&mut tree, info, border, "CornerRadius", "6");
  set_text(&mut tree, info, border, "Background", "#3B82F6");

  let border = tree.get_as::<Border>(border).expect("a Border node");
  assert_eq!(border.padding.get(), Thickness::new(4.0, 8.0, 4.0, 8.0));
  assert_eq!(border.corner_radius.get(), CornerRadius::uniform(6.0));
  assert_eq!(border.background.get(), Some(Brush::from(Color::rgb(0x3B, 0x82, 0xF6))));
}

// A `TextBlock` takes a `String` and an enum of its own, and a number, a brush and an enum through
// the groups it holds.
#[test]
fn text_block_values_parse_from_text() {
  let mut tree = UiTree::new();
  let info = TextBlock::TYPE_INFO;
  let block = create(&mut tree, info);

  set_text(&mut tree, info, block, "Text", "Hello");
  set_text(&mut tree, info, block, "TextAlignment", "Center");
  set_text(&mut tree, info, block, "FontSize", "18");
  set_text(&mut tree, info, block, "Foreground", "Red");
  set_text(&mut tree, info, block, "HorizontalAlignment", "Right");

  let block = tree.get_as::<TextBlock>(block).expect("a TextBlock node");
  assert_eq!(block.text.get(), "Hello");
  assert_eq!(block.text_alignment.get(), TextAlignment::Center);
  assert_eq!(block.format.font_size.get(), 18.0);
  assert_eq!(block.format.foreground.get(), "Red".parse::<Brush>().expect("a color name"));
  assert_eq!(block.layout.horizontal_alignment.get(), HorizontalAlignment::Right);
}

// One `HorizontalAlignment` access serves every control: the layout group's key reaches the
// property on a `Border` and on a `TextBlock`.
#[test]
fn a_group_property_has_one_access_for_every_control() {
  let mut tree = UiTree::new();
  let border = create(&mut tree, Border::TYPE_INFO);
  let block = create(&mut tree, TextBlock::TYPE_INFO);
  let on_border = Border::TYPE_INFO
    .property("HorizontalAlignment")
    .expect("in the layout group");
  let on_block = TextBlock::TYPE_INFO
    .property("HorizontalAlignment")
    .expect("in the layout group");
  assert!(std::ptr::eq(on_border.info, on_block.info));

  let alignment = on_border.info.access.expect("the access feature is on");
  for node in [border, block] {
    (alignment.set_text)(&mut tree, node, "Center", Precedence::Local).expect("Center is an alignment");
  }

  assert_eq!(
    tree
      .get_as::<Border>(border)
      .map(|border| border.layout.horizontal_alignment.get()),
    Some(HorizontalAlignment::Center)
  );
  assert_eq!(
    tree
      .get_as::<TextBlock>(block)
      .map(|block| block.layout.horizontal_alignment.get()),
    Some(HorizontalAlignment::Center)
  );
}

/// The access of `property` on `type_info`.
fn access(type_info: &'static TypeInfo, property: &str) -> &'static PropertyAccess {
  let info = type_info.property(property).expect("the property is registered").info;
  info.access.expect("the access feature is on")
}

// A brush written as an element is staged, given its members, and moved into `Background`.
#[test]
fn a_staged_brush_becomes_the_background() {
  let mut tree = UiTree::new();
  let border = create(&mut tree, Border::TYPE_INFO);
  let solid = create(&mut tree, SolidColorBrush::TYPE_INFO);
  (access(SolidColorBrush::TYPE_INFO, "Color").set_text)(&mut tree, solid, "#3B82F6", Precedence::Local).expect("a color");

  (access(Border::TYPE_INFO, "Background").set_object)(&mut tree, border, solid, Precedence::Local).expect("a brush");

  assert_eq!(
    tree.get_as::<Border>(border).map(|border| border.background.get()),
    Some(Some(Brush::from(Color::rgb(0x3B, 0x82, 0xF6))))
  );
  assert!(!tree.contains(solid));
}

// A gradient takes its stops as staged values, in order, and becomes the brush it describes.
#[test]
fn a_staged_gradient_takes_its_stops() {
  let mut tree = UiTree::new();
  let border = create(&mut tree, Border::TYPE_INFO);
  let gradient = create(&mut tree, LinearGradientBrush::TYPE_INFO);
  let Some(ContentInfo::Collection { add_child, .. }) = LinearGradientBrush::TYPE_INFO.content else {
    panic!("a gradient's content is its stops");
  };
  for (offset, color) in [("0", "White"), ("1", "SteelBlue")] {
    let stop = create(&mut tree, GradientStop::TYPE_INFO);
    (access(GradientStop::TYPE_INFO, "Offset").set_text)(&mut tree, stop, offset, Precedence::Local).expect("a number");
    (access(GradientStop::TYPE_INFO, "Color").set_text)(&mut tree, stop, color, Precedence::Local).expect("a color");
    add_child(&mut tree, gradient, stop).expect("a stop");
  }

  (access(Border::TYPE_INFO, "Background").set_object)(&mut tree, border, gradient, Precedence::Local).expect("a brush");

  let expected = LinearGradientBrush {
    stops: [
      GradientStop::new(0.0, Color::WHITE),
      GradientStop::new(1.0, Color::STEEL_BLUE),
    ]
    .into(),
    ..LinearGradientBrush::default()
  };
  assert_eq!(
    tree.get_as::<Border>(border).map(|border| border.background.get()),
    Some(Some(Brush::from(expected)))
  );
}
