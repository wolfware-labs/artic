//! The value types layout measures and positions with, in logical pixels.

mod parse;
mod path;
mod relative;
mod transform;

use crate::finite;
pub use parse::{GeometryProblem, ParseGeometryError};
pub use path::{FillRule, Path, PathBuilder, PathElement};
pub use relative::{RelativePoint, RelativeScalar, Scaling};
use std::fmt::{Display, Formatter, Result};
use std::ops::{Add, Sub};
pub use transform::Transform;

/// A width and a height, without a position.
///
/// `Size::INFINITY` stands for "no limit" in a measure constraint.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Size {
  /// The horizontal extent.
  pub width: f32,
  /// The vertical extent.
  pub height: f32,
}

impl Size {
  /// No width and no height.
  pub const ZERO: Size = Size::new(0.0, 0.0);
  /// Unlimited in both directions, for measure constraints.
  pub const INFINITY: Size = Size::new(f32::INFINITY, f32::INFINITY);

  /// Creates a size of `width` by `height`.
  #[must_use]
  pub const fn new(width: f32, height: f32) -> Size {
    Size { width, height }
  }
}

/// A position.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Point {
  /// The horizontal coordinate, growing to the right.
  pub x: f32,
  /// The vertical coordinate, growing downward.
  pub y: f32,
}

impl Point {
  /// The origin.
  pub const ZERO: Point = Point::new(0.0, 0.0);

  /// Creates the point at `x`, `y`.
  #[must_use]
  pub const fn new(x: f32, y: f32) -> Point {
    Point { x, y }
  }
}

/// A rectangle: a position and a size.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
  /// The left edge.
  pub x: f32,
  /// The top edge.
  pub y: f32,
  /// The horizontal extent.
  pub width: f32,
  /// The vertical extent.
  pub height: f32,
}

impl Rect {
  /// An empty rectangle at the origin.
  pub const ZERO: Rect = Rect::new(0.0, 0.0, 0.0, 0.0);

  /// Creates the rectangle at `x`, `y` of `width` by `height`.
  #[must_use]
  pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect { x, y, width, height }
  }

  /// The top-left corner.
  #[must_use]
  pub const fn origin(self) -> Point {
    Point::new(self.x, self.y)
  }

  /// The width and height.
  #[must_use]
  pub const fn size(self) -> Size {
    Size::new(self.width, self.height)
  }

  /// Shrinks the rectangle by `thickness` on each side, as padding does to the space for content.
  ///
  /// The width and height stop at zero when the thickness is larger than the rectangle.
  #[must_use]
  pub fn deflate(self, thickness: Thickness) -> Rect {
    Rect::new(
      finite::add(self.x, thickness.left),
      finite::add(self.y, thickness.top),
      finite::add(self.width, -thickness.horizontal()).max(0.0),
      finite::add(self.height, -thickness.vertical()).max(0.0),
    )
  }

  /// This rectangle with its edges on the device-pixel grid of a display at `scale` device pixels
  /// per logical pixel.
  ///
  /// Each edge is rounded on its own and the size is what lies between them, so rectangles that
  /// share an edge keep sharing it: three columns of `33.33` become `33`, `34` and `33` wide,
  /// without a gap.
  #[must_use]
  pub fn snap(self, scale: f32) -> Rect {
    let edge = |value: f32| {
      let snapped = (value * scale).round() / scale;
      if snapped.is_finite() { snapped } else { value }
    };
    let (left, top) = (edge(self.x), edge(self.y));
    let (right, bottom) = (edge(finite::add(self.x, self.width)), edge(finite::add(self.y, self.height)));
    Rect::new(left, top, finite::add(right, -left), finite::add(bottom, -top))
  }
}

/// The width of space on each side of a box, such as a margin, a padding or a border.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Thickness {
  /// The space on the left.
  pub left: f32,
  /// The space at the top.
  pub top: f32,
  /// The space on the right.
  pub right: f32,
  /// The space at the bottom.
  pub bottom: f32,
}

impl Thickness {
  /// No space on any side.
  pub const ZERO: Thickness = Thickness::uniform(0.0);

  /// Creates a thickness with each side given separately.
  #[must_use]
  pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Thickness {
    Thickness {
      left,
      top,
      right,
      bottom,
    }
  }

  /// Creates a thickness with the same width on every side.
  #[must_use]
  pub const fn uniform(width: f32) -> Thickness {
    Thickness::new(width, width, width, width)
  }

  /// Creates a thickness with `horizontal` on the left and right and `vertical` at the top and
  /// bottom.
  #[must_use]
  pub const fn symmetric(horizontal: f32, vertical: f32) -> Thickness {
    Thickness::new(horizontal, vertical, horizontal, vertical)
  }

  /// The total width it adds across: left plus right.
  #[must_use]
  pub const fn horizontal(self) -> f32 {
    finite::add(self.left, self.right)
  }

  /// The total height it adds: top plus bottom.
  #[must_use]
  pub const fn vertical(self) -> f32 {
    finite::add(self.top, self.bottom)
  }

  /// The same thickness with every side that isn't a finite number of 0 or more set to 0, the
  /// rule borders and padding follow.
  #[must_use]
  pub fn usable(self) -> Thickness {
    let side = |side: f32| if side.is_finite() && side > 0.0 { side } else { 0.0 };
    Thickness::new(side(self.left), side(self.top), side(self.right), side(self.bottom))
  }
}

impl Add<Thickness> for Size {
  type Output = Size;

  /// Grows the size by the thickness on each side, as a margin does to a desired size.
  fn add(self, thickness: Thickness) -> Size {
    Size::new(
      finite::add(self.width, thickness.horizontal()),
      finite::add(self.height, thickness.vertical()),
    )
  }
}

impl Sub<Thickness> for Size {
  type Output = Size;

  /// Shrinks the size by the thickness on each side, stopping at zero, as padding does to the space
  /// left for content.
  fn sub(self, thickness: Thickness) -> Size {
    Size::new(
      finite::add(self.width, -thickness.horizontal()).max(0.0),
      finite::add(self.height, -thickness.vertical()).max(0.0),
    )
  }
}

impl From<f32> for Thickness {
  /// The same width on every side, as one number in text is.
  fn from(width: f32) -> Thickness {
    Thickness::uniform(width)
  }
}

/// The radius of each corner of a box, clockwise from the top-left.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CornerRadius {
  /// The top-left radius.
  pub top_left: f32,
  /// The top-right radius.
  pub top_right: f32,
  /// The bottom-right radius.
  pub bottom_right: f32,
  /// The bottom-left radius.
  pub bottom_left: f32,
}

impl CornerRadius {
  /// Square corners.
  pub const ZERO: CornerRadius = CornerRadius::uniform(0.0);

  /// Creates a corner radius with each corner given separately, clockwise from the top-left.
  #[must_use]
  pub const fn new(top_left: f32, top_right: f32, bottom_right: f32, bottom_left: f32) -> CornerRadius {
    CornerRadius {
      top_left,
      top_right,
      bottom_right,
      bottom_left,
    }
  }

  /// Creates a corner radius with the same radius on every corner.
  #[must_use]
  pub const fn uniform(radius: f32) -> CornerRadius {
    CornerRadius::new(radius, radius, radius, radius)
  }
}

impl From<f32> for CornerRadius {
  /// The same radius on every corner, as one number in text is.
  fn from(radius: f32) -> CornerRadius {
    CornerRadius::uniform(radius)
  }
}

/// A rectangle with rounded corners.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RoundedRect {
  /// The rectangle the corners are cut from.
  pub rect: Rect,
  /// The radius of each corner.
  pub radius: CornerRadius,
}

impl RoundedRect {
  /// Creates `rect` with corners of `radius`.
  #[must_use]
  pub const fn new(rect: Rect, radius: CornerRadius) -> RoundedRect {
    RoundedRect { rect, radius }
  }

  /// The same shape with radii that can be drawn: a negative, NaN or infinite radius is `0`, and
  /// when two radii on one side add up to more than the side, all four shrink by the same factor,
  /// as in CSS.
  #[must_use]
  pub fn fitted(self) -> RoundedRect {
    let drawable = |radius: f32| {
      if radius > 0.0 && radius.is_finite() {
        f64::from(radius)
      } else {
        0.0
      }
    };
    let CornerRadius {
      top_left,
      top_right,
      bottom_right,
      bottom_left,
    } = self.radius;
    let [top_left, top_right, bottom_right, bottom_left] = [top_left, top_right, bottom_right, bottom_left].map(drawable);
    let (width, height) = (f64::from(self.rect.width.max(0.0)), f64::from(self.rect.height.max(0.0)));
    let scale = [
      (width, top_left + top_right),
      (width, bottom_left + bottom_right),
      (height, top_left + bottom_left),
      (height, top_right + bottom_right),
    ]
    .into_iter()
    .filter(|&(side, radii)| radii > side)
    .map(|(side, radii)| side / radii)
    .fold(1.0, f64::min);
    let [top_left, top_right, bottom_right, bottom_left] =
      [top_left, top_right, bottom_right, bottom_left].map(|radius| (radius * scale) as f32);
    RoundedRect::new(self.rect, CornerRadius::new(top_left, top_right, bottom_right, bottom_left))
  }
}

const KAPPA: f32 = 0.552_284_8;

impl RoundedRect {
  /// The [`fitted`](RoundedRect::fitted) shape with radii a border of `thickness` can follow
  /// inside: each corner's radius is at most the distance from its corner to the inner edge of
  /// each side opposite it. [`inner`](RoundedRect::inner) and [`ring`](RoundedRect::ring) start
  /// from this outline, so a background filled with it shares their edge. A side that isn't a
  /// finite number of 0 or more counts as 0, and without a border the shape is `fitted`.
  #[must_use]
  pub fn fitted_for_border(self, thickness: Thickness) -> RoundedRect {
    let fitted = self.fitted();
    let thickness = thickness.usable();
    let (width, height) = (fitted.rect.width.max(0.0), fitted.rect.height.max(0.0));
    let reach = |radius: f32, across: f32, down: f32| {
      radius
        .min(finite::add(width, -across))
        .min(finite::add(height, -down))
        .max(0.0)
    };
    let CornerRadius {
      top_left,
      top_right,
      bottom_right,
      bottom_left,
    } = fitted.radius;
    RoundedRect::new(
      fitted.rect,
      CornerRadius::new(
        reach(top_left, thickness.right, thickness.bottom),
        reach(top_right, thickness.left, thickness.bottom),
        reach(bottom_right, thickness.left, thickness.top),
        reach(bottom_left, thickness.right, thickness.top),
      ),
    )
  }

  /// The [`fitted`](RoundedRect::fitted) outline as a closed path, built with the same curves as
  /// [`ring`](RoundedRect::ring)'s outer figure. The ring's outline is
  /// [`fitted_for_border`](RoundedRect::fitted_for_border), so a fill of
  /// `shape.fitted_for_border(thickness).path()` meets `shape.ring(thickness)` exactly.
  #[must_use]
  pub fn path(self) -> Path {
    outline(self.fitted()).build()
  }

  /// The shape inside a border of `thickness`, as CSS defines its inner border edge: the
  /// [`fitted_for_border`](RoundedRect::fitted_for_border) rectangle deflated by the thickness,
  /// each corner's radius reduced per axis by its two sides' widths, stopping at 0. Each inner
  /// corner keeps its outer corner's center, so the inner edge stays inside the outline. A side
  /// that isn't a finite number of 0 or more counts as 0. When no room is left inside, a single
  /// point, which as a clip hides everything.
  #[must_use]
  pub fn inner(self, thickness: Thickness) -> Path {
    let thickness = thickness.usable();
    let outer = self.fitted_for_border(thickness);
    let rect = outer.rect.deflate(thickness);
    if rect.width <= 0.0 || rect.height <= 0.0 {
      return Path::builder(rect.origin()).build();
    }
    let CornerRadius {
      top_left,
      top_right,
      bottom_right,
      bottom_left,
    } = outer.radius;
    let shrink = |radius: f32, side: f32| (radius - side).max(0.0);
    let radii = [
      (shrink(top_left, thickness.left), shrink(top_left, thickness.top)),
      (shrink(top_right, thickness.right), shrink(top_right, thickness.top)),
      (shrink(bottom_right, thickness.right), shrink(bottom_right, thickness.bottom)),
      (shrink(bottom_left, thickness.left), shrink(bottom_left, thickness.bottom)),
    ];
    contour(Path::builder(start(rect, &radii)), rect, &radii).build()
  }

  /// The border between this shape's [`fitted_for_border`](RoundedRect::fitted_for_border)
  /// outline and its [`inner`](RoundedRect::inner) shape, as one even-odd path: the outline and
  /// the inner edge are two figures, so one fill draws the ring, whatever the sides' widths. When
  /// no room is left inside, the whole outline.
  #[must_use]
  pub fn ring(self, thickness: Thickness) -> Path {
    let mut builder = outline(self.fitted_for_border(thickness));
    let inner = self.inner(thickness);
    if inner.elements().len() > 1 {
      for element in inner.elements() {
        builder = match *element {
          PathElement::MoveTo(point) => builder.move_to(point),
          PathElement::LineTo(point) => builder.line_to(point),
          PathElement::QuadTo(control, end) => builder.quad_to(control, end),
          PathElement::CubicTo(first, second, end) => builder.cubic_to(first, second, end),
          PathElement::Close => builder.close(),
        };
      }
    }
    builder.fill_rule(FillRule::EvenOdd).build()
  }
}

fn outline(shape: RoundedRect) -> PathBuilder {
  let CornerRadius {
    top_left,
    top_right,
    bottom_right,
    bottom_left,
  } = shape.radius;
  let radii = [
    (top_left, top_left),
    (top_right, top_right),
    (bottom_right, bottom_right),
    (bottom_left, bottom_left),
  ];
  contour(Path::builder(start(shape.rect, &radii)), shape.rect, &radii)
}

fn start(rect: Rect, radii: &[(f32, f32); 4]) -> Point {
  Point::new(finite::add(rect.x, radii[0].0), rect.y)
}

fn contour(builder: PathBuilder, rect: Rect, radii: &[(f32, f32); 4]) -> PathBuilder {
  let (left, top) = (rect.x, rect.y);
  let (right, bottom) = (finite::add(rect.x, rect.width), finite::add(rect.y, rect.height));
  let [top_left, top_right, bottom_right, bottom_left] = *radii;
  let corners = [
    (
      Point::new(finite::add(right, -top_right.0), top),
      Point::new(right, top),
      Point::new(right, finite::add(top, top_right.1)),
    ),
    (
      Point::new(right, finite::add(bottom, -bottom_right.1)),
      Point::new(right, bottom),
      Point::new(finite::add(right, -bottom_right.0), bottom),
    ),
    (
      Point::new(finite::add(left, bottom_left.0), bottom),
      Point::new(left, bottom),
      Point::new(left, finite::add(bottom, -bottom_left.1)),
    ),
    (
      Point::new(left, finite::add(top, top_left.1)),
      Point::new(left, top),
      Point::new(finite::add(left, top_left.0), top),
    ),
  ];
  corners
    .into_iter()
    .fold(builder, |builder, (from, at, to)| corner(builder.line_to(from), from, at, to))
    .close()
}

fn corner(builder: PathBuilder, from: Point, at: Point, to: Point) -> PathBuilder {
  if from == at || to == at {
    return builder.line_to(at).line_to(to);
  }
  let toward = |point: Point| Point::new(point.x + (at.x - point.x) * KAPPA, point.y + (at.y - point.y) * KAPPA);
  builder.cubic_to(toward(from), toward(to), to)
}

impl From<Rect> for RoundedRect {
  /// The rectangle with square corners.
  fn from(rect: Rect) -> RoundedRect {
    RoundedRect::new(rect, CornerRadius::ZERO)
  }
}

impl Display for Size {
  /// Writes `width,height`.
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    write!(formatter, "{},{}", self.width, self.height)
  }
}

impl Display for Point {
  /// Writes `x,y`.
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    write!(formatter, "{},{}", self.x, self.y)
  }
}

impl Display for Rect {
  /// Writes `x,y,width,height`.
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    write!(formatter, "{},{},{},{}", self.x, self.y, self.width, self.height)
  }
}

impl Display for Thickness {
  /// Writes `left,top,right,bottom`.
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    write!(formatter, "{},{},{},{}", self.left, self.top, self.right, self.bottom)
  }
}

impl Display for CornerRadius {
  /// Writes `top_left,top_right,bottom_right,bottom_left`.
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    write!(
      formatter,
      "{},{},{},{}",
      self.top_left, self.top_right, self.bottom_right, self.bottom_left
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  fn box_with(radius: f32) -> RoundedRect {
    RoundedRect::new(Rect::new(0.0, 0.0, 100.0, 50.0), CornerRadius::uniform(radius))
  }

  // Each inner corner shrinks by its two sides' widths, per axis: 10 with left 4 and top 2 gives a
  // top-left corner 6 wide and 8 tall.
  #[test]
  fn inner_radii_shrink_per_axis() {
    let inner = box_with(10.0).inner(Thickness::new(4.0, 2.0, 0.0, 0.0));

    assert_eq!(inner.elements()[0], PathElement::MoveTo(Point::new(10.0, 2.0)));
    assert!(inner.elements().contains(&PathElement::LineTo(Point::new(4.0, 10.0))));
  }

  // With no thickness, the inner shape is the outline itself.
  #[test]
  fn zero_thickness_inner_is_the_outline() {
    let inner = box_with(10.0).inner(Thickness::ZERO);

    assert_eq!(inner.elements()[0], PathElement::MoveTo(Point::new(10.0, 0.0)));
    assert_eq!(inner.bounds(), Rect::new(0.0, 0.0, 100.0, 50.0));
  }

  // A side wider than the corner's radius leaves a square inner corner.
  #[test]
  fn inner_radius_stops_at_zero() {
    let inner = box_with(2.0).inner(Thickness::uniform(4.0));

    assert!(
      !inner
        .elements()
        .iter()
        .any(|element| matches!(element, PathElement::CubicTo(..)))
    );
  }

  // A corner that would reach past the inner edge of an opposite side shrinks to that edge: 19 on
  // a box 20 wide with a right side of 10 becomes 10, and the other corners keep theirs.
  #[test]
  fn radii_stop_at_the_opposite_inner_edge() {
    let outer = RoundedRect::new(Rect::new(0.0, 0.0, 20.0, 40.0), CornerRadius::new(19.0, 1.0, 0.0, 0.0));

    let fitted = outer.fitted_for_border(Thickness::new(0.0, 0.0, 10.0, 0.0));

    assert_eq!(fitted.radius, CornerRadius::new(10.0, 1.0, 0.0, 0.0));
  }

  // Each inner corner keeps its outer corner's center, so the inner edge never crosses the
  // outline: with no top or left side, the top-left corners of both figures are the same curve.
  #[test]
  fn inner_corners_share_the_outer_centers() {
    let outer = RoundedRect::new(Rect::new(0.0, 0.0, 20.0, 40.0), CornerRadius::new(19.0, 1.0, 0.0, 0.0));
    let thickness = Thickness::new(0.0, 0.0, 10.0, 0.0);

    let ring = outer.ring(thickness);
    let inner = outer.inner(thickness);

    let close = ring
      .elements()
      .iter()
      .position(|element| *element == PathElement::Close)
      .expect("the outline closes");
    let top_left = |elements: &[PathElement]| {
      elements
        .iter()
        .rev()
        .find(|element| matches!(element, PathElement::CubicTo(..)))
        .copied()
    };
    assert_eq!(ring.elements()[0], PathElement::MoveTo(Point::new(10.0, 0.0)));
    assert_eq!(top_left(inner.elements()), top_left(&ring.elements()[..close]));
  }

  // Over a grid of radii and uneven sides, every point on the inner edge lies inside the outline,
  // so the even-odd ring never paints outside the box.
  #[test]
  fn inner_stays_inside_the_outline() {
    let inside = |outer: RoundedRect, point: Point| {
      let Rect { x, y, width, height } = outer.rect;
      let CornerRadius {
        top_left,
        top_right,
        bottom_right,
        bottom_left,
      } = outer.radius;
      let corners = [
        (
          top_left,
          x + top_left,
          y + top_left,
          point.x < x + top_left && point.y < y + top_left,
        ),
        (
          top_right,
          x + width - top_right,
          y + top_right,
          point.x > x + width - top_right && point.y < y + top_right,
        ),
        (
          bottom_right,
          x + width - bottom_right,
          y + height - bottom_right,
          point.x > x + width - bottom_right && point.y > y + height - bottom_right,
        ),
        (
          bottom_left,
          x + bottom_left,
          y + height - bottom_left,
          point.x < x + bottom_left && point.y > y + height - bottom_left,
        ),
      ];
      let slack = 1e-3;
      point.x >= x - slack
        && point.y >= y - slack
        && point.x <= x + width + slack
        && point.y <= y + height + slack
        && corners
          .iter()
          .all(|&(radius, cx, cy, within)| !within || (point.x - cx).hypot(point.y - cy) <= radius * (1.0 + 3e-4) + slack)
    };
    let mid = |from: Point, first: Point, second: Point, to: Point| {
      Point::new(
        (from.x + 3.0 * first.x + 3.0 * second.x + to.x) / 8.0,
        (from.y + 3.0 * first.y + 3.0 * second.y + to.y) / 8.0,
      )
    };
    let rect = Rect::new(0.0, 0.0, 20.0, 40.0);
    for radii in [
      [19.0, 1.0, 0.0, 0.0],
      [10.0; 4],
      [20.0, 0.0, 20.0, 0.0],
      [5.0, 15.0, 2.0, 18.0],
    ] {
      for sides in [
        [0.0, 0.0, 10.0, 0.0],
        [1.0, 2.0, 3.0, 4.0],
        [8.0, 1.0, 0.0, 12.0],
        [0.0, 15.0, 9.0, 0.0],
      ] {
        let [top_left, top_right, bottom_right, bottom_left] = radii;
        let [left, top, right, bottom] = sides;
        let shape = RoundedRect::new(rect, CornerRadius::new(top_left, top_right, bottom_right, bottom_left));
        let thickness = Thickness::new(left, top, right, bottom);
        let outer = shape.fitted_for_border(thickness);
        let mut current = Point::ZERO;
        for element in shape.inner(thickness).elements() {
          let point = match *element {
            PathElement::MoveTo(point) | PathElement::LineTo(point) => point,
            PathElement::CubicTo(first, second, to) => {
              assert!(inside(outer, mid(current, first, second, to)), "{radii:?} {sides:?}");
              to
            }
            PathElement::QuadTo(_, to) => to,
            PathElement::Close => current,
          };
          assert!(inside(outer, point), "{radii:?} {sides:?} {point}");
          current = point;
        }
      }
    }
  }

  // Without a border, fitting for one changes nothing.
  #[test]
  fn fitted_for_no_border_is_fitted() {
    let outer = RoundedRect::new(Rect::new(0.0, 0.0, 20.0, 40.0), CornerRadius::new(19.0, 1.0, 30.0, 0.0));

    assert_eq!(outer.fitted_for_border(Thickness::ZERO), outer.fitted());
  }

  // A side that isn't a finite number of 0 or more becomes 0; the others stay.
  #[test]
  fn usable_thickness_zeroes_invalid_sides() {
    let thickness = Thickness::new(-1.0, f32::NAN, f32::INFINITY, 3.0);

    assert_eq!(thickness.usable(), Thickness::new(0.0, 0.0, 0.0, 3.0));
  }

  // The border-fitted outline path is the ring's outer figure, curve for curve, so a background
  // filled with it meets the ring exactly, even where the border cuts a radius: 10 on a 20 box with
  // sides of 15 becomes 5.
  #[test]
  fn path_is_the_ring_outline() {
    let shape = RoundedRect::new(Rect::new(0.0, 0.0, 20.0, 20.0), CornerRadius::uniform(10.0));
    let thickness = Thickness::new(15.0, 0.0, 0.0, 0.0);
    let ring = shape.ring(thickness);
    let close = ring
      .elements()
      .iter()
      .position(|element| *element == PathElement::Close)
      .expect("closes");
    let outline = shape.fitted_for_border(thickness);

    assert_eq!(outline.radius.top_right, 5.0);
    assert_eq!(outline.path().elements(), &ring.elements()[..=close]);
    assert_ne!(shape.path().elements(), &ring.elements()[..=close]);
  }

  // Radii far larger than the box are fitted to it before anything else.
  #[test]
  fn huge_radii_are_fitted_first() {
    let ring = RoundedRect::new(Rect::new(0.0, 0.0, 20.0, 20.0), CornerRadius::uniform(1000.0)).ring(Thickness::uniform(2.0));

    assert_eq!(ring.bounds(), Rect::new(0.0, 0.0, 20.0, 20.0));
    assert!(ring.is_finite());
  }

  // A border wider than half the box leaves no hole: the inner shape is a point and the ring is
  // the whole box.
  #[test]
  fn thick_border_leaves_no_hole() {
    let outer = box_with(10.0);

    let inner = outer.inner(Thickness::uniform(30.0));
    let ring = outer.ring(Thickness::uniform(30.0));

    assert_eq!(inner.elements().len(), 1);
    assert_eq!(
      ring
        .elements()
        .iter()
        .filter(|element| matches!(element, PathElement::MoveTo(_)))
        .count(),
      1
    );
  }

  // The ring is two figures filled even-odd, so the inside stays empty.
  #[test]
  fn ring_fills_even_odd() {
    let ring = box_with(10.0).ring(Thickness::uniform(2.0));

    assert_eq!(ring.fill_rule(), FillRule::EvenOdd);
    assert_eq!(
      ring
        .elements()
        .iter()
        .filter(|element| matches!(element, PathElement::MoveTo(_)))
        .count(),
      2
    );
  }

  // The ring covers exactly the outer shape's box.
  #[test]
  fn ring_bounds_match_the_outer_shape() {
    assert_eq!(
      box_with(10.0).ring(Thickness::new(4.0, 2.0, 6.0, 8.0)).bounds(),
      Rect::new(0.0, 0.0, 100.0, 50.0)
    );
  }

  // A negative, NaN or infinite side counts as no side.
  #[test]
  fn negative_sides_count_as_zero() {
    let expected = box_with(10.0).inner(Thickness::ZERO);

    for side in [-5.0, f32::NAN, f32::INFINITY] {
      assert_eq!(box_with(10.0).inner(Thickness::uniform(side)), expected, "{side}");
    }
  }

  // Snapped edges land on whole device pixels at any scale, and neighbors still share their edge.
  #[test]
  fn snap_rounds_edges_to_device_pixels() {
    for scale in [1.0_f32, 1.25, 2.0] {
      let columns: Vec<Rect> = (0..3)
        .map(|index| Rect::new(index as f32 * 100.0 / 3.0, 0.3, 100.0 / 3.0, 10.4).snap(scale))
        .collect();

      for rect in &columns {
        for edge in [rect.x, rect.y, rect.x + rect.width, rect.y + rect.height] {
          let device = edge * scale;
          assert!((device - device.round()).abs() < 1e-3, "{rect} at scale {scale}");
        }
      }
      for pair in columns.windows(2) {
        assert_eq!(pair[0].x + pair[0].width, pair[1].x, "no gap at scale {scale}");
      }
    }
    let widths: Vec<f32> = (0..3)
      .map(|index| Rect::new(index as f32 * 100.0 / 3.0, 0.0, 100.0 / 3.0, 1.0).snap(1.0).width)
      .collect();
    assert_eq!(widths, [33.0, 34.0, 33.0]);
  }

  // Each type's zero constant is its default.
  #[test]
  fn zero_constants_are_the_defaults() {
    assert_eq!(Thickness::ZERO, Thickness::default());
    assert_eq!(CornerRadius::ZERO, CornerRadius::default());
    assert_eq!(Size::ZERO, Size::default());
    assert_eq!(Point::ZERO, Point::default());
    assert_eq!(Rect::ZERO, Rect::default());
  }

  // The shorthand constructors spread their values over the sides and corners.
  #[test]
  fn shorthand_constructors() {
    assert_eq!(Thickness::uniform(4.0), Thickness::new(4.0, 4.0, 4.0, 4.0));
    assert_eq!(Thickness::symmetric(4.0, 8.0), Thickness::new(4.0, 8.0, 4.0, 8.0));
    assert_eq!(CornerRadius::uniform(2.0), CornerRadius::new(2.0, 2.0, 2.0, 2.0));
  }

  // Display writes every component, comma-separated, in field order.
  #[test]
  fn display_writes_the_canonical_form() {
    assert_eq!(Thickness::symmetric(4.0, 8.5).to_string(), "4,8.5,4,8.5");
    assert_eq!(CornerRadius::new(1.0, 2.0, 3.0, 4.0).to_string(), "1,2,3,4");
    assert_eq!(Rect::new(1.0, 2.0, 30.0, 40.0).to_string(), "1,2,30,40");
    assert_eq!(Size::new(30.0, 40.0).to_string(), "30,40");
    assert_eq!(Point::new(-1.5, 2.0).to_string(), "-1.5,2");
  }

  // A rectangle splits into its origin and size.
  #[test]
  fn rect_origin_and_size() {
    let rect = Rect::new(1.0, 2.0, 30.0, 40.0);

    assert_eq!(rect.origin(), Point::new(1.0, 2.0));
    assert_eq!(rect.size(), Size::new(30.0, 40.0));
  }

  // Radii that two corners of one side can't share shrink together, by the factor the tightest
  // side needs, as in CSS, however large they are; radii that fit are kept, and negative, NaN or
  // infinite ones are 0.
  #[test]
  fn rounded_rect_fitted_scales_overlapping_radii() {
    let rect = Rect::new(0.0, 0.0, 10.0, 40.0);
    let fitted = |radius| RoundedRect::new(rect, radius).fitted().radius;

    assert_eq!(fitted(CornerRadius::uniform(5.0)), CornerRadius::uniform(5.0));
    assert_eq!(
      fitted(CornerRadius::new(10.0, 10.0, 0.0, 0.0)),
      CornerRadius::new(5.0, 5.0, 0.0, 0.0)
    );
    assert_eq!(
      fitted(CornerRadius::new(4.0, 0.0, 20.0, 0.0)),
      CornerRadius::new(2.0, 0.0, 10.0, 0.0)
    );
    assert_eq!(
      fitted(CornerRadius::new(-1.0, f32::NAN, f32::INFINITY, 3.0)),
      CornerRadius::new(0.0, 0.0, 0.0, 3.0)
    );
    assert_eq!(RoundedRect::new(rect, CornerRadius::uniform(5.0)).fitted().rect, rect);
    assert_eq!(
      fitted(CornerRadius::new(f32::MAX, f32::MAX, 0.0, 0.0)),
      CornerRadius::new(5.0, 5.0, 0.0, 0.0)
    );
  }

  // A rectangle is a rounded rectangle with square corners.
  #[test]
  fn rect_converts_into_rounded_rect() {
    let rect = Rect::new(1.0, 2.0, 3.0, 4.0);

    assert_eq!(RoundedRect::from(rect), RoundedRect::new(rect, CornerRadius::ZERO));
  }

  // Adding a thickness grows a size by both sides; subtracting shrinks it and stops at zero.
  #[test]
  fn size_arithmetic_with_thickness() {
    let padding = Thickness::new(1.0, 2.0, 3.0, 4.0);

    assert_eq!(Size::new(10.0, 10.0) + padding, Size::new(14.0, 16.0));
    assert_eq!(Size::new(10.0, 10.0) - padding, Size::new(6.0, 4.0));
    assert_eq!(Size::new(3.0, 3.0) - padding, Size::ZERO);
    assert_eq!(Size::INFINITY - padding, Size::INFINITY);
  }

  // Deflating moves the origin in by the left and top widths and shrinks, stopping at zero.
  #[test]
  fn rect_deflate() {
    let padding = Thickness::new(1.0, 2.0, 3.0, 4.0);

    assert_eq!(
      Rect::new(10.0, 10.0, 20.0, 20.0).deflate(padding),
      Rect::new(11.0, 12.0, 16.0, 14.0)
    );
    assert_eq!(Rect::new(0.0, 0.0, 2.0, 2.0).deflate(padding), Rect::new(1.0, 2.0, 0.0, 0.0));
  }

  // One number is every side or corner, as `Padding="8"` and `CornerRadius="4"` are.
  #[test]
  fn one_number_converts_to_every_side_and_corner() {
    assert_eq!(Thickness::from(8.0), Thickness::uniform(8.0));
    assert_eq!(CornerRadius::from(4.0), CornerRadius::uniform(4.0));
  }
}
