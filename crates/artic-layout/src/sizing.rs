use crate::finite;
use crate::geometry::{Rect, Size, Thickness};
use artic_macros::XamlEnum;

/// Where an element sits across its slot when it's narrower than the slot.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, XamlEnum)]
pub enum HorizontalAlignment {
  /// Against the slot's left edge.
  Left,
  /// Centered.
  Center,
  /// Against the slot's right edge.
  Right,
  /// As wide as the slot, unless an explicit or maximum width stops it, then centered.
  #[default]
  Stretch,
}

/// Where an element sits along its slot's height when it's shorter than the slot.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, XamlEnum)]
pub enum VerticalAlignment {
  /// Against the slot's top edge.
  Top,
  /// Centered.
  Center,
  /// Against the slot's bottom edge.
  Bottom,
  /// As tall as the slot, unless an explicit or maximum height stops it, then centered.
  #[default]
  Stretch,
}

/// The axis a panel stacks or wraps its children along.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, XamlEnum)]
pub enum Orientation {
  /// Left to right.
  Horizontal,
  /// Top to bottom.
  #[default]
  Vertical,
}

/// Everything about an element's size and position that isn't its own measuring and arranging:
/// what [`measure_with`] and [`arrange_with`] apply around it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sizing {
  /// Space kept free around the element.
  pub margin: Thickness,
  /// An explicit width, or `None` to size to the content.
  pub width: Option<f32>,
  /// An explicit height, or `None` to size to the content.
  pub height: Option<f32>,
  /// The smallest size, which wins over an explicit or maximum one.
  pub min: Size,
  /// The largest size.
  pub max: Size,
  /// Where the element sits across its slot.
  pub horizontal: HorizontalAlignment,
  /// Where the element sits along its slot's height.
  pub vertical: VerticalAlignment,
}

impl Default for Sizing {
  fn default() -> Sizing {
    Sizing {
      margin: Thickness::ZERO,
      width: None,
      height: None,
      min: Size::ZERO,
      max: Size::INFINITY,
      horizontal: HorizontalAlignment::default(),
      vertical: VerticalAlignment::default(),
    }
  }
}

impl Sizing {
  /// The smallest and largest size allowed, once the explicit size is taken into account: an
  /// explicit size is clamped between the minimum and the maximum, and fixes both.
  fn limits(&self) -> (Size, Size) {
    let (min_width, max_width) = limits(self.width, self.min.width, self.max.width);
    let (min_height, max_height) = limits(self.height, self.min.height, self.max.height);
    (Size::new(min_width, min_height), Size::new(max_width, max_height))
  }
}

fn limits(explicit: Option<f32>, min: f32, max: f32) -> (f32, f32) {
  match explicit {
    Some(value) => {
      let value = value.min(max).max(min);
      (value, value)
    }
    None => (min, max.max(min)),
  }
}

/// Measures an element with `sizing` around its own measuring, `inner`, and returns its desired
/// size, margin included.
///
/// `inner` is offered `available` without the margin, narrowed to the element's limits; what it
/// returns is held within those limits, then the margin is added back.
///
/// # Examples
///
/// ```
/// use artic_layout::geometry::{Size, Thickness};
/// use artic_layout::{Sizing, measure_with};
///
/// let sizing = Sizing { margin: Thickness::uniform(10.0), width: Some(100.0), ..Sizing::default() };
/// let desired = measure_with(&sizing, Size::new(400.0, 300.0), |available| {
///   assert_eq!(available, Size::new(100.0, 280.0));
///   Size::new(60.0, 20.0)
/// });
/// assert_eq!(desired, Size::new(120.0, 40.0));
/// ```
pub fn measure_with(sizing: &Sizing, available: Size, inner: impl FnOnce(Size) -> Size) -> Size {
  let (min, max) = sizing.limits();
  let margin = sizing.margin;
  let offered = Size::new(
    finite::add(available.width, -margin.horizontal())
      .max(0.0)
      .min(max.width)
      .max(min.width),
    finite::add(available.height, -margin.vertical())
      .max(0.0)
      .min(max.height)
      .max(min.height),
  );
  let desired = inner(offered);
  Size::new(
    finite::add(desired.width.max(min.width).min(max.width), margin.horizontal()),
    finite::add(desired.height.max(min.height).min(max.height), margin.vertical()),
  )
}

/// The rectangle an element arranges its content in, within the `slot` its parent gave it, given
/// the `desired` size its measure returned, margin included.
///
/// The margin is taken off the slot; a stretching axis takes the rest and any other alignment
/// takes the desired size, both held within the element's limits; the result is placed in the
/// slot by the alignment.
///
/// # Examples
///
/// ```
/// use artic_layout::geometry::{Rect, Size, Thickness};
/// use artic_layout::{HorizontalAlignment, Sizing, VerticalAlignment, arrange_with};
///
/// let sizing = Sizing {
///   margin: Thickness::uniform(10.0),
///   horizontal: HorizontalAlignment::Center,
///   vertical: VerticalAlignment::Top,
///   ..Sizing::default()
/// };
/// let inner = arrange_with(&sizing, Rect::new(0.0, 0.0, 400.0, 300.0), Size::new(120.0, 40.0));
/// assert_eq!(inner, Rect::new(150.0, 10.0, 100.0, 20.0));
/// ```
#[must_use]
pub fn arrange_with(sizing: &Sizing, slot: Rect, desired: Size) -> Rect {
  let (min, max) = sizing.limits();
  let margin = sizing.margin;
  let space = slot.deflate(margin);
  let content = desired - margin;
  let stretch_width = sizing.horizontal == HorizontalAlignment::Stretch && sizing.width.is_none();
  let stretch_height = sizing.vertical == VerticalAlignment::Stretch && sizing.height.is_none();
  let width = if stretch_width { space.width } else { content.width }
    .max(min.width)
    .min(max.width);
  let height = if stretch_height { space.height } else { content.height }
    .max(min.height)
    .min(max.height);
  let x = match sizing.horizontal {
    HorizontalAlignment::Left => space.x,
    HorizontalAlignment::Right => finite::add(finite::add(space.x, space.width), -width),
    HorizontalAlignment::Center | HorizontalAlignment::Stretch => finite::add(space.x, finite::add(space.width, -width) / 2.0),
  };
  let y = match sizing.vertical {
    VerticalAlignment::Top => space.y,
    VerticalAlignment::Bottom => finite::add(finite::add(space.y, space.height), -height),
    VerticalAlignment::Center | VerticalAlignment::Stretch => finite::add(space.y, finite::add(space.height, -height) / 2.0),
  };
  Rect::new(x, y, width, height)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::text::{Paragraph, ParagraphStyle, TextEngine, TextLimits, TextStyle};
  use crate::{ArrangeCx, LayoutManager, LayoutNodes, Layoutable, MeasureCx, PixelGrid};
  use artic_core::Object;
  use artic_core::property::AnyProperty;
  use artic_core::tree::{NodeId, UiTree};
  use pretty_assertions::assert_eq;
  use std::cell::Cell;

  /// A text engine for nodes that shape no text.
  struct NoText;

  impl TextEngine for NoText {
    fn paragraph(&mut self, _text: &str, _style: &TextStyle, _paragraph: &ParagraphStyle, _limits: TextLimits) -> Paragraph {
      Paragraph::default()
    }
  }

  const SLOT: Rect = Rect::new(0.0, 0.0, 400.0, 300.0);

  // The alignments parse through `XamlEnum`, ignoring case, with an error that lists the names.
  #[test]
  fn alignments_parse_with_the_derive() {
    assert_eq!("left".parse::<HorizontalAlignment>(), Ok(HorizontalAlignment::Left));
    assert_eq!("BOTTOM".parse::<VerticalAlignment>(), Ok(VerticalAlignment::Bottom));
    let error = "middle".parse::<HorizontalAlignment>().expect_err("not a name");
    assert_eq!(error.expected, ["Left", "Center", "Right", "Stretch"]);
    assert_eq!(error.type_name, "HorizontalAlignment");
  }

  // `Orientation` parses ignoring case and is vertical by default.
  #[test]
  fn orientation_parses_ignoring_case() {
    assert_eq!("horizontal".parse::<Orientation>(), Ok(Orientation::Horizontal));
    assert_eq!("Vertical".parse::<Orientation>(), Ok(Orientation::Vertical));
    assert_eq!(Orientation::default(), Orientation::Vertical);
  }

  // Alignments parse from their names in any case, and an unknown name lists the valid ones.
  #[test]
  fn alignments_parse_case_insensitively() {
    assert_eq!("left".parse::<HorizontalAlignment>(), Ok(HorizontalAlignment::Left));
    assert_eq!("Stretch".parse::<HorizontalAlignment>(), Ok(HorizontalAlignment::Stretch));
    assert_eq!("BOTTOM".parse::<VerticalAlignment>(), Ok(VerticalAlignment::Bottom));

    let error = "middle".parse::<VerticalAlignment>().unwrap_err();
    assert_eq!(error.expected, ["Top", "Center", "Bottom", "Stretch"]);
  }

  /// Arranges content of 60 × 20 plus `margin` with `sizing`.
  fn arranged(sizing: Sizing) -> Rect {
    let desired = Size::new(60.0, 20.0) + sizing.margin;
    arrange_with(&sizing, SLOT, desired)
  }

  // Each alignment places the content against its edge, in the middle, or across the slot.
  #[test]
  fn alignments_place_the_content() {
    let at = |horizontal, vertical| {
      arranged(Sizing {
        horizontal,
        vertical,
        ..Sizing::default()
      })
    };
    use HorizontalAlignment as H;
    use VerticalAlignment as V;

    assert_eq!(at(H::Left, V::Top), Rect::new(0.0, 0.0, 60.0, 20.0));
    assert_eq!(at(H::Center, V::Center), Rect::new(170.0, 140.0, 60.0, 20.0));
    assert_eq!(at(H::Right, V::Bottom), Rect::new(340.0, 280.0, 60.0, 20.0));
    assert_eq!(at(H::Stretch, V::Stretch), SLOT);
  }

  // An explicit size beats stretching, and the content is then centered.
  #[test]
  fn explicit_size_beats_stretch() {
    let rect = arranged(Sizing {
      width: Some(100.0),
      ..Sizing::default()
    });

    assert_eq!(rect, Rect::new(150.0, 0.0, 100.0, 300.0));
  }

  // The minimum wins over the maximum and the explicit size, in measure and in arrange.
  #[test]
  fn limits_hold_and_min_wins() {
    let sizing = Sizing {
      width: Some(50.0),
      min: Size::new(80.0, 0.0),
      max: Size::new(70.0, 25.0),
      ..Sizing::default()
    };

    let desired = measure_with(&sizing, SLOT.size(), |available| {
      assert_eq!(available, Size::new(80.0, 25.0));
      Size::new(10.0, 300.0)
    });
    assert_eq!(desired, Size::new(80.0, 25.0));
    assert_eq!(arranged(sizing).size(), Size::new(80.0, 25.0));
  }

  // An infinite available size stays infinite for the content, and the margin can't make it
  // negative.
  #[test]
  fn measure_keeps_infinity_and_stops_at_zero() {
    let sizing = Sizing {
      margin: Thickness::uniform(30.0),
      ..Sizing::default()
    };

    measure_with(&sizing, Size::new(f32::INFINITY, 40.0), |available| {
      assert_eq!(available, Size::new(f32::INFINITY, 0.0));
      Size::ZERO
    });
  }

  /// An element using the helpers, as `Layout` does: it records the rectangle its own
  /// arranging gets.
  #[derive(Debug)]
  struct Element {
    sizing: Sizing,
    inner_rect: Cell<Option<Rect>>,
  }

  impl Object for Element {
    fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  }

  impl Layoutable for Element {
    fn measure(&self, _cx: &mut MeasureCx<'_>, available: Size) -> Size {
      measure_with(&self.sizing, available, |_| Size::new(60.0, 20.0))
    }

    fn arrange(&self, cx: &mut ArrangeCx<'_>, rect: Rect) {
      let desired = cx.desired(cx.node());
      self.inner_rect.set(Some(arrange_with(&self.sizing, rect, desired)));
    }
  }

  struct Elements;

  impl LayoutNodes for Elements {
    fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable> {
      tree.get_as::<Element>(node).map(|element| element as &dyn Layoutable)
    }
  }

  // Through a layout run, the margin and alignment are applied before the element arranges its
  // content: what it gets is the content's rectangle, not the slot.
  #[test]
  fn margin_and_alignment_applied_before_arrange_override() {
    let mut tree = UiTree::new();
    let sizing = Sizing {
      margin: Thickness::new(10.0, 20.0, 10.0, 20.0),
      horizontal: HorizontalAlignment::Right,
      vertical: VerticalAlignment::Center,
      ..Sizing::default()
    };
    let element = tree.alloc(Box::new(Element {
      sizing,
      inner_rect: Cell::new(None),
    }));

    let grid = PixelGrid { scale: 1.0, snap: false };
    LayoutManager::default().run(&mut tree, &Elements, element, SLOT.size(), grid, &mut NoText);

    let inner = tree.get_as::<Element>(element).expect("the element").inner_rect.get();
    assert_eq!(inner, Some(Rect::new(330.0, 140.0, 60.0, 20.0)));
  }
}
