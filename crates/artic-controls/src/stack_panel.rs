use crate::Panel;
use artic_core::property::Property;
use artic_core::tree::NodeId;
use artic_element::{Children, ElementImpl, RenderCx};
use artic_layout::finite;
use artic_layout::geometry::{Rect, Size};
use artic_layout::{ArrangeCx, MeasureCx, Orientation};
use artic_macros::element;
use artic_render::SceneBuilder;

/// Lines its children up in a column or a row, with `Spacing` between them, and draws its
/// background under them.
#[element]
#[derive(Debug)]
pub struct StackPanel {
  /// The background.
  #[ui(flatten)]
  pub panel: Panel,
  /// The children, in order.
  #[ui(content)]
  pub children: Children,
  /// The axis the children are lined up along. Vertical by default.
  #[ui(affects = layout)]
  pub orientation: Property<Orientation>,
  /// The space between two children that take part in layout. A value that isn't a finite
  /// number of 0 or more counts as 0.
  #[ui(affects = layout)]
  pub spacing: Property<f32>,
}

#[derive(Debug, Clone, Copy)]
struct Axis(Orientation);

impl Axis {
  fn split(self, size: Size) -> (f32, f32) {
    match self.0 {
      Orientation::Vertical => (size.height, size.width),
      Orientation::Horizontal => (size.width, size.height),
    }
  }

  fn join(self, along: f32, across: f32) -> Size {
    match self.0 {
      Orientation::Vertical => Size::new(across, along),
      Orientation::Horizontal => Size::new(along, across),
    }
  }

  fn slot(self, start: f32, along: f32, across: f32) -> Rect {
    match self.0 {
      Orientation::Vertical => Rect::new(0.0, start, across, along),
      Orientation::Horizontal => Rect::new(start, 0.0, along, across),
    }
  }
}

impl ElementImpl for StackPanel {
  fn measure_override(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
    let axis = Axis(self.orientation.get());
    let (_, across) = axis.split(available);
    let children: Vec<NodeId> = cx.children().collect();
    let mut along = 0.0;
    let mut widest = 0.0_f32;
    for (index, child) in children.into_iter().enumerate() {
      if index > 0 {
        along = finite::add(along, finite::usable(self.spacing.get()));
      }
      let (child_along, child_across) = axis.split(cx.measure(child, axis.join(f32::INFINITY, across)));
      along = finite::add(along, child_along);
      widest = widest.max(child_across);
    }
    axis.join(along, widest)
  }

  fn arrange_override(&self, cx: &mut ArrangeCx<'_>, size: Size) {
    let axis = Axis(self.orientation.get());
    let (_, across) = axis.split(size);
    let children: Vec<NodeId> = cx.children().collect();
    let mut start = 0.0;
    for (index, child) in children.into_iter().enumerate() {
      if index > 0 {
        start = finite::add(start, finite::usable(self.spacing.get()));
      }
      let (along, _) = axis.split(cx.desired(child));
      cx.arrange(child, axis.slot(start, along, across));
      start = finite::add(start, along);
    }
  }

  fn render(&self, cx: &mut RenderCx<'_>, scene: &mut SceneBuilder) {
    self.panel.render(cx, scene);
  }
}
