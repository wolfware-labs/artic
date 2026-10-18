//! The property groups every control has.

use crate::{AccessCx, Classes, Cursor, HitTestCx, HitTestResult, Visibility};
use artic_core::property::Property;
use artic_layout::geometry::{Point, Rect, Size, Thickness, Transform};
use artic_layout::{ArrangeCx, HorizontalAlignment, MeasureCx, Sizing, VerticalAlignment, arrange_with, measure_with};
use artic_macros::UiProperties;
use artic_render::{Layers, Shape, drawable_opacity};

/// Where an element sits and how big it is: size, limits, margin and alignment.
///
/// A control has it in its `layout` field. [`measure`](Layout::measure) and
/// [`arrange`](Layout::arrange) apply it around the control's own layout.
#[derive(Debug, UiProperties)]
pub struct Layout {
  /// An explicit width, or `None` to size to the content.
  #[ui(affects = layout)]
  pub width: Property<Option<f32>>,
  /// An explicit height, or `None` to size to the content.
  #[ui(affects = layout)]
  pub height: Property<Option<f32>>,
  /// The smallest width, which wins over an explicit or maximum one.
  #[ui(affects = layout)]
  pub min_width: Property<f32>,
  /// The smallest height, which wins over an explicit or maximum one.
  #[ui(affects = layout)]
  pub min_height: Property<f32>,
  /// The largest width.
  #[ui(default = f32::INFINITY, affects = layout)]
  pub max_width: Property<f32>,
  /// The largest height.
  #[ui(default = f32::INFINITY, affects = layout)]
  pub max_height: Property<f32>,
  /// Space kept free around the element.
  #[ui(default = Thickness::ZERO, affects = layout)]
  pub margin: Property<Thickness>,
  /// Where the element sits across its slot.
  #[ui(affects = layout)]
  pub horizontal_alignment: Property<HorizontalAlignment>,
  /// Where the element sits along its slot's height.
  #[ui(affects = layout)]
  pub vertical_alignment: Property<VerticalAlignment>,
}

/// How an element is drawn: visibility, opacity, transform and order among its siblings.
///
/// A control has it in its `appearance` field.
#[derive(Debug, UiProperties)]
pub struct Appearance {
  /// Whether the element is shown, hidden in its space, or collapsed to no space.
  #[ui(affects = layout | render)]
  pub visibility: Property<Visibility>,
  /// How opaque the element and its descendants are, from 0 to 1.
  #[ui(default = 1.0, affects = render)]
  pub opacity: Property<f32>,
  /// A transform applied when drawing, which layout doesn't see.
  #[ui(affects = render)]
  pub render_transform: Property<Option<Transform>>,
  /// The drawing and hit-testing order among siblings.
  #[ui(affects = render)]
  pub z_index: Property<i32>,
}

/// How an element takes input: enabled, hit-testable, focusable, the pointer over it and its
/// tooltip.
///
/// A control has it in its `interaction` field.
#[derive(Debug, UiProperties)]
pub struct Interaction {
  /// Whether the element takes input; a disabled ancestor disables it whatever it sets itself.
  #[ui(default = true, affects = render, inherits = and)]
  pub is_enabled: Property<bool>,
  /// Whether the element can be hit by the pointer while it's drawn.
  #[ui(default = true, affects = render)]
  pub is_hit_test_visible: Property<bool>,
  /// Whether the element can take keyboard focus.
  #[ui(affects = render)]
  pub focusable: Property<bool>,
  /// The pointer shown over the element.
  #[ui]
  pub cursor: Property<Cursor>,
  /// Text shown when the pointer rests on the element.
  #[ui]
  pub tool_tip: Property<Option<String>>,
}

/// What style selectors and code find an element by: its name and style classes.
///
/// A control has it in its `identity` field.
#[derive(Debug, UiProperties)]
pub struct Identity {
  /// The element's name, which style selectors and code can find it by.
  #[ui]
  pub name: Property<Option<String>>,
  /// The style classes style selectors match.
  #[ui]
  pub classes: Property<Classes>,
}

impl Layout {
  /// The size properties and alignments, for [`measure_with`] and [`arrange_with`].
  #[must_use]
  pub fn sizing(&self) -> Sizing {
    Sizing {
      margin: self.margin.get(),
      width: self.width.get(),
      height: self.height.get(),
      min: Size::new(self.min_width.get(), self.min_height.get()),
      max: Size::new(self.max_width.get(), self.max_height.get()),
      horizontal: self.horizontal_alignment.get(),
      vertical: self.vertical_alignment.get(),
    }
  }

  /// Measures the element: `content` measures within `available` less the margin and held within
  /// the limits, and its result, held within the limits, gets the margin back. A collapsed
  /// element never gets here: the layout manager leaves it out.
  pub fn measure(
    &self,
    cx: &mut MeasureCx<'_>,
    available: Size,
    content: impl FnOnce(&mut MeasureCx<'_>, Size) -> Size,
  ) -> Size {
    measure_with(&self.sizing(), available, |inner| content(cx, inner))
  }

  /// Arranges the element in `slot`: its bounds are the slot less the margin, sized and aligned,
  /// and `content` arranges the children within their size. A collapsed element never gets
  /// here: the layout manager leaves it out.
  pub fn arrange(&self, cx: &mut ArrangeCx<'_>, slot: Rect, content: impl FnOnce(&mut ArrangeCx<'_>, Size)) {
    let desired = cx.desired(cx.node());
    let bounds = cx.set_bounds(arrange_with(&self.sizing(), slot, desired));
    content(cx, bounds.size());
  }
}

impl Appearance {
  /// The element's layers in composition for its arranged `size`: its opacity held to 0–1 (NaN
  /// counts as 1), its render transform around its centre in its own coordinates, whether it's
  /// visible, its z-index, and the control's `clip`.
  #[must_use]
  pub fn layers(&self, size: Size, clip: Option<Shape>) -> Layers {
    let (x, y) = (size.width / 2.0, size.height / 2.0);
    Layers {
      transform: self
        .render_transform
        .get()
        .map(|transform| Transform::translate(-x, -y).then(transform).then(Transform::translate(x, y))),
      clip,
      opacity: drawable_opacity(self.opacity.get()),
      visible: self.visibility.get() == Visibility::Visible,
      z_index: self.z_index.get(),
    }
  }
}

impl Interaction {
  /// Hit-tests the element at `point`: a miss when `visibility`, from the appearance group, isn't
  /// `Visible` or `IsHitTestVisible` is false; otherwise whether `point` is inside its bounds.
  #[must_use]
  pub fn hit_test(&self, cx: &HitTestCx<'_>, visibility: Visibility, point: Point) -> HitTestResult {
    if visibility != Visibility::Visible || !self.is_hit_test_visible.get() {
      return HitTestResult::Miss;
    }
    cx.bounds_test(point)
  }

  /// Whether the element gets input: `IsEnabled`, which a disabled logical ancestor makes false.
  #[must_use]
  pub fn accepts_input(&self) -> bool {
    self.is_enabled.get()
  }

  /// The element's default accessibility node, which the control's `ElementImpl::accessibility`
  /// adds to: a generic container at its bounds, disabled when `IsEnabled` is false. The tree walk
  /// leaves out elements that aren't visible.
  #[must_use]
  pub fn accessibility(&self, cx: &AccessCx<'_>) -> accesskit::Node {
    let bounds = cx.bounds();
    let mut node = accesskit::Node::new(accesskit::Role::GenericContainer);
    node.set_bounds(accesskit::Rect::new(
      f64::from(bounds.x),
      f64::from(bounds.y),
      f64::from(bounds.x + bounds.width),
      f64::from(bounds.y + bounds.height),
    ));
    if !self.is_enabled.get() {
      node.set_disabled();
    }
    node
  }
}
