use artic_core::Object;
use artic_core::property::PropertyVisitor;
use artic_element::{AccessCx, Element, RenderCx};
use artic_layout::geometry::{Rect, Size};
use artic_layout::{ArrangeCx, Layoutable, MeasureCx};
use artic_render::SceneBuilder;
use std::cell::Cell;

/// A leaf element that asks for the same size whatever is available and draws nothing. It counts
/// how often it's measured and arranged.
#[derive(Debug)]
pub struct Fixed {
  size: Cell<Size>,
  measured: Cell<u32>,
  arranged: Cell<u32>,
}

impl Fixed {
  /// A leaf that asks for `width` × `height`.
  #[must_use]
  pub fn new(width: f32, height: f32) -> Fixed {
    Fixed {
      size: Cell::new(Size::new(width, height)),
      measured: Cell::new(0),
      arranged: Cell::new(0),
    }
  }

  /// Changes the size it asks for. Layout sees it once the node's measure is invalidated.
  pub fn set_size(&self, size: Size) {
    self.size.set(size);
  }

  /// How often it was measured.
  #[must_use]
  pub fn measured(&self) -> u32 {
    self.measured.get()
  }

  /// How often it was arranged.
  #[must_use]
  pub fn arranged(&self) -> u32 {
    self.arranged.get()
  }
}

impl Object for Fixed {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

impl Element for Fixed {
  fn render(&self, _cx: &mut RenderCx<'_>, _scene: &mut SceneBuilder) {}

  fn accessibility(&self, _cx: &AccessCx<'_>) -> accesskit::Node {
    accesskit::Node::new(accesskit::Role::GenericContainer)
  }
}

artic_element::element_type!(Fixed);

impl Layoutable for Fixed {
  fn measure(&self, _cx: &mut MeasureCx<'_>, _available: Size) -> Size {
    self.measured.set(self.measured.get() + 1);
    self.size.get()
  }

  fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: Rect) {
    self.arranged.set(self.arranged.get() + 1);
  }
}

/// Places its children top to bottom, each as wide as the stack and as tall as it asked. It asks
/// for the widest child's width and the sum of the heights, and counts its calls like [`Fixed`].
#[derive(Debug, Default)]
pub struct Stack {
  measured: Cell<u32>,
  arranged: Cell<u32>,
}

impl Stack {
  /// An empty stack.
  #[must_use]
  pub fn new() -> Stack {
    Stack::default()
  }

  /// How often it was measured.
  #[must_use]
  pub fn measured(&self) -> u32 {
    self.measured.get()
  }

  /// How often it was arranged.
  #[must_use]
  pub fn arranged(&self) -> u32 {
    self.arranged.get()
  }
}

impl Object for Stack {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

impl Element for Stack {
  fn render(&self, _cx: &mut RenderCx<'_>, _scene: &mut SceneBuilder) {}

  fn accessibility(&self, _cx: &AccessCx<'_>) -> accesskit::Node {
    accesskit::Node::new(accesskit::Role::GenericContainer)
  }
}

artic_element::element_type!(Stack);

impl Layoutable for Stack {
  fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
    self.measured.set(self.measured.get() + 1);
    let mut total = Size::ZERO;
    for child in cx.children() {
      let desired = cx.measure(child, Size::new(available.width, f32::INFINITY));
      total = Size::new(total.width.max(desired.width), total.height + desired.height);
    }
    total
  }

  fn arrange(&self, cx: &mut ArrangeCx<'_>, rect: Rect) {
    self.arranged.set(self.arranged.get() + 1);
    let mut top = 0.0;
    for child in cx.children() {
      let height = cx.desired(child).height;
      cx.arrange(child, Rect::new(0.0, top, rect.width, height));
      top += height;
    }
  }
}
