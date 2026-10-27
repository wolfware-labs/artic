// A hand-written element laid out through LayoutManager, found by way of the cast table, as the
// frame pipeline will.

use artic_core::Object;
use artic_core::property::PropertyVisitor;
use artic_core::tree::{TreeKind, UiTree};
use artic_element::{AccessCx, ElementImpl, ElementTypes, RenderCx};
use artic_layout::geometry::{Rect, Size};
use artic_layout::{ArrangeCx, LayoutManager, Layoutable, MeasureCx, PixelGrid};
use artic_render::SceneBuilder;
use artic_text::Engine;
use pretty_assertions::assert_eq;

/// A panel that stacks its children vertically, written as a control would be: its own layout in
/// `ElementImpl`, and the `Element` outline by hand, as `#[element]` will write it.
#[derive(Debug)]
struct Column;

impl Object for Column {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

impl ElementImpl for Column {
  fn measure_override(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
    let children: Vec<_> = cx.children().collect();
    children.iter().fold(Size::ZERO, |total, child| {
      let size = cx.measure(*child, available);
      Size::new(total.width.max(size.width), total.height + size.height)
    })
  }

  fn arrange_override(&self, cx: &mut ArrangeCx<'_>, size: Size) {
    let children: Vec<_> = cx.children().collect();
    let mut y = 0.0;
    for child in children {
      let height = cx.desired(child).height;
      cx.arrange(child, Rect::new(0.0, y, size.width, height));
      y += height;
    }
  }
}

impl Layoutable for Column {
  fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
    self.measure_override(cx, available)
  }

  fn arrange(&self, cx: &mut ArrangeCx<'_>, rect: Rect) {
    self.arrange_override(cx, rect.size());
  }
}

impl artic_element::Element for Column {
  fn render(&self, _cx: &mut RenderCx<'_>, _scene: &mut SceneBuilder) {}

  fn accessibility(&self, _cx: &AccessCx<'_>) -> accesskit::Node {
    accesskit::Node::new(accesskit::Role::GenericContainer)
  }
}

artic_element::element_type!(Column);

/// A fixed-size leaf that measures itself.
#[derive(Debug)]
struct Block(f32);

impl Object for Block {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

impl Layoutable for Block {
  fn measure(&self, _cx: &mut MeasureCx<'_>, _available: Size) -> Size {
    Size::new(30.0, self.0)
  }

  fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: Rect) {}
}

impl artic_element::Element for Block {
  fn render(&self, _cx: &mut RenderCx<'_>, _scene: &mut SceneBuilder) {}

  fn accessibility(&self, _cx: &AccessCx<'_>) -> accesskit::Node {
    accesskit::Node::new(accesskit::Role::GenericContainer)
  }
}

artic_element::element_type!(Block);

/// A control with nothing special: its measure is ElementImpl's default, which takes no space.
#[derive(Debug)]
struct Spacer;

impl Object for Spacer {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

impl ElementImpl for Spacer {}

impl Layoutable for Spacer {
  fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
    self.measure_override(cx, available)
  }

  fn arrange(&self, cx: &mut ArrangeCx<'_>, rect: Rect) {
    self.arrange_override(cx, rect.size());
  }
}

impl artic_element::Element for Spacer {
  fn render(&self, _cx: &mut RenderCx<'_>, _scene: &mut SceneBuilder) {}

  fn accessibility(&self, _cx: &AccessCx<'_>) -> accesskit::Node {
    accesskit::Node::new(accesskit::Role::GenericContainer)
  }
}

artic_element::element_type!(Spacer);

/// A node type that was never registered as an element.
#[derive(Debug)]
struct Unregistered;

impl Object for Unregistered {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

// A registered column stacks its registered children; an unregistered node and a spacer using
// ElementImpl's default measure take no space, so the second block sits right under the first.
#[test]
fn registered_elements_are_laid_out_through_the_cast_table() {
  let mut tree = UiTree::new();
  let column = tree.alloc(Box::new(Column));
  let first = tree.alloc(Box::new(Block(20.0)));
  let unregistered = tree.alloc(Box::new(Unregistered));
  let spacer = tree.alloc(Box::new(Spacer));
  let second = tree.alloc(Box::new(Block(10.0)));
  for child in [first, unregistered, spacer, second] {
    tree.add_child(TreeKind::Visual, column, child);
  }
  let types = ElementTypes::from_link_time();
  let mut layout = LayoutManager::default();

  layout.run(
    &mut tree,
    &types,
    column,
    Size::new(200.0, 100.0),
    PixelGrid { scale: 1.0, snap: false },
    &mut Engine::with_fonts([]),
  );

  assert_eq!(layout.desired(column), Some(Size::new(30.0, 30.0)));
  assert_eq!(layout.rect(first), Some(Rect::new(0.0, 0.0, 200.0, 20.0)));
  assert_eq!(layout.rect(unregistered), Some(Rect::new(0.0, 20.0, 200.0, 0.0)));
  assert_eq!(layout.desired(spacer), Some(Size::ZERO));
  assert_eq!(layout.rect(second), Some(Rect::new(0.0, 20.0, 200.0, 10.0)));
}

// A stale id has no layoutable node; asking doesn't panic.
#[test]
fn stale_ids_have_no_element() {
  use artic_layout::LayoutNodes;
  let mut tree = UiTree::new();
  let block = tree.alloc(Box::new(Block(5.0)));
  tree.free(block);
  let types = ElementTypes::from_link_time();

  assert!(types.layoutable(&tree, block).is_none());
}

/// Calls `element_type!` where `Element`, `Any` and `linkme` are other items and there is no
/// prelude, so it compiles only if the macro names everything by its full path.
#[no_implicit_prelude]
mod hygiene {
  #[allow(dead_code)]
  struct Element;
  #[allow(dead_code)]
  struct Any;
  #[allow(dead_code)]
  mod linkme {}

  ::artic_element::element_type!(super::Block);
}
