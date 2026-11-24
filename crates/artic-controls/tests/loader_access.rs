// A UI language's runtime loader writes the M1 controls' properties through the registry's
// property access, knowing only names and text: one value of each kind the controls hold.

use artic_controls::{Border, TextBlock};
use artic_core::property::Precedence;
use artic_core::tree::{NodeId, UiTree};
use artic_layout::HorizontalAlignment;
use artic_layout::geometry::{CornerRadius, Thickness};
use artic_layout::text::TextAlignment;
use artic_registry::{Registry, TypeInfo, UiType};
use artic_render::{Brush, Color};
use pretty_assertions::assert_eq;

/// Sets `property` of `node`, whose type `type_info` describes, from `text` as the loader does:
/// looked up by name, with no value type in sight.
fn set_text(tree: &mut UiTree, type_info: &'static TypeInfo, node: NodeId, property: &str, text: &str) {
  let found = type_info.property(property).expect("the property is registered");
  (found.info.access.set_text)(tree, node, text, Precedence::Local).expect("the text parses");
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

  for node in [border, block] {
    (on_border.info.access.set_text)(&mut tree, node, "Center", Precedence::Local).expect("Center is an alignment");
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
