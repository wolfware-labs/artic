// A UI language's runtime loader writes the controls' properties through the registry's
// property access, knowing only names and text: one value of each kind the controls hold.

use artic_controls::{Border, TextBlock, TextElement};
use artic_core::property::Precedence;
use artic_core::tree::{NodeId, UiTree};
use artic_element::Desktop;
use artic_layout::HorizontalAlignment;
use artic_layout::geometry::{CornerRadius, Thickness};
use artic_layout::text::TextAlignment;
use artic_registry::{ContentInfo, PropertyAccess, Registry, TypeInfo, UiType, ValueError};
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

/// The access of the attached property `name` that `owner` declares.
fn attached_access(owner: &'static TypeInfo, name: &str) -> &'static PropertyAccess {
  let info = owner.attached_property(name).expect("the attached property is registered");
  info.access.expect("the access feature is on")
}

// `TextElement.FontSize` on a `Border`, which has no field for it, is stored as the border's
// attached value.
#[test]
fn an_attached_value_is_stored_on_a_node_without_its_field() {
  let mut tree = UiTree::new();
  let border = create(&mut tree, Border::TYPE_INFO);

  (attached_access(TextElement::TYPE_INFO, "FontSize").set_text)(&mut tree, border, "20", Precedence::Local).expect("a number");

  assert_eq!(TextElement::font_size(&tree, border), 20.0);
  assert!(tree.property(border, TextElement::FONT_SIZE.key()).is_some());
}

// `TextElement.FontSize` on a `TextBlock` lands in its `TextFormat` field, which shares the key.
#[test]
fn an_attached_value_lands_in_the_field_that_shares_its_key() {
  let mut tree = UiTree::new();
  let block = create(&mut tree, TextBlock::TYPE_INFO);

  (attached_access(TextElement::TYPE_INFO, "FontSize").set_text)(&mut tree, block, "20", Precedence::Local).expect("a number");

  assert_eq!(
    tree.get_as::<TextBlock>(block).map(|block| block.format.font_size.get()),
    Some(20.0)
  );
}

// An optional attached property, `Desktop.Width`, is set to a number and then to null.
#[test]
fn an_optional_attached_value_is_set_to_null() {
  let mut tree = UiTree::new();
  let border = create(&mut tree, Border::TYPE_INFO);
  let width = attached_access(Desktop::TYPE_INFO, "Width");

  (width.set_text)(&mut tree, border, "800", Precedence::Local).expect("a number");
  assert_eq!(Desktop::width(&tree, border), Some(800.0));
  (width.set_null)(&mut tree, border, Precedence::Local).expect("an optional width");

  assert_eq!(Desktop::width(&tree, border), None);
}

// A node no longer in the tree is refused by a set and by a clear.
#[test]
fn an_attached_write_refuses_a_freed_node() {
  let mut tree = UiTree::new();
  let border = create(&mut tree, Border::TYPE_INFO);
  tree.free(border);
  let font_size = attached_access(TextElement::TYPE_INFO, "FontSize");

  assert_eq!(
    (font_size.set_text)(&mut tree, border, "20", Precedence::Local),
    Err(ValueError::WrongNode)
  );
  assert_eq!(
    (font_size.clear)(&mut tree, border, Precedence::Local),
    Err(ValueError::WrongNode)
  );
}

// Clearing an attached value a node never held succeeds and stores nothing on it.
#[test]
fn clearing_an_attached_value_never_held_stores_nothing() {
  let mut tree = UiTree::new();
  let border = create(&mut tree, Border::TYPE_INFO);

  assert_eq!(
    (attached_access(TextElement::TYPE_INFO, "FontSize").clear)(&mut tree, border, Precedence::Local),
    Ok(())
  );
  assert!(tree.property(border, TextElement::FONT_SIZE.key()).is_none());
}

// Text that doesn't parse as the attached property's type is refused with the parser's message.
#[test]
fn attached_text_that_does_not_parse_is_refused() {
  let mut tree = UiTree::new();
  let border = create(&mut tree, Border::TYPE_INFO);

  let refused = (attached_access(TextElement::TYPE_INFO, "FontSize").set_text)(&mut tree, border, "big", Precedence::Local);

  assert_eq!(
    refused,
    Err(ValueError::Parse {
      expected: "f32",
      message: "big".parse::<f32>().expect_err("not a number").to_string(),
    })
  );
}

// A number-valued attached property refuses a node and a staged object, which is taken out of the
// tree.
#[test]
fn an_attached_number_refuses_a_node_and_an_object() {
  let mut tree = UiTree::new();
  let border = create(&mut tree, Border::TYPE_INFO);
  let child = create(&mut tree, TextBlock::TYPE_INFO);
  let brush = create(&mut tree, SolidColorBrush::TYPE_INFO);
  let font_size = attached_access(TextElement::TYPE_INFO, "FontSize");

  assert_eq!(
    (font_size.set_node)(&mut tree, border, child, Precedence::Local),
    Err(ValueError::WrongType {
      expected: "f32",
      found: "an element",
    })
  );
  assert_eq!(
    (font_size.set_object)(&mut tree, border, brush, Precedence::Local),
    Err(ValueError::WrongType {
      expected: "f32",
      found: "an object",
    })
  );
  assert!(!tree.contains(brush));
}
