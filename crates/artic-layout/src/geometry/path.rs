use crate::geometry::{Point, Rect};
use std::sync::Arc;

/// One step of a [`Path`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathElement {
  /// Starts a figure at the point.
  MoveTo(Point),
  /// A straight line to the point.
  LineTo(Point),
  /// A quadratic Bézier curve, pulled toward the control point, to the end point.
  QuadTo(Point, Point),
  /// A cubic Bézier curve, pulled toward two control points, to the end point.
  CubicTo(Point, Point, Point),
  /// A straight line back to the figure's start, which closes it.
  Close,
}

impl PathElement {
  fn points(self) -> impl Iterator<Item = Point> {
    let (points, count) = match self {
      PathElement::MoveTo(point) | PathElement::LineTo(point) => ([point; 3], 1),
      PathElement::QuadTo(control, end) => ([control, end, end], 2),
      PathElement::CubicTo(first, second, end) => ([first, second, end], 3),
      PathElement::Close => ([Point::ZERO; 3], 0),
    };
    points.into_iter().take(count)
  }
}

/// Which areas of a [`Path`] are inside it where its outline crosses itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FillRule {
  /// A point is inside when a ray from it crosses the outline an odd number of times, so where
  /// figures overlap they cancel out.
  #[default]
  EvenOdd,
  /// A point is inside when the outline winds around it, in either direction, so overlaps stay
  /// filled.
  NonZero,
}

/// An outline of straight lines and curves, in one or more figures, with the rule that decides
/// what's inside it.
///
/// A fill or a clip treats an open figure as closed by a straight line back to its start; a stroke
/// leaves it open. Cloning a path shares its elements rather than copying them.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
  elements: Arc<[PathElement]>,
  fill_rule: FillRule,
}

impl Path {
  /// Starts a path whose first figure begins at `start`.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_layout::geometry::{FillRule, Path, Point};
  ///
  /// let triangle = Path::builder(Point::new(50.0, 0.0))
  ///   .line_to(Point::new(100.0, 100.0))
  ///   .line_to(Point::new(0.0, 100.0))
  ///   .close()
  ///   .fill_rule(FillRule::NonZero)
  ///   .build();
  /// assert_eq!(triangle.elements().len(), 4);
  /// ```
  #[must_use]
  pub fn builder(start: Point) -> PathBuilder {
    PathBuilder {
      elements: vec![PathElement::MoveTo(start)],
      start,
      closed: false,
      fill_rule: FillRule::default(),
    }
  }

  /// The elements, in order. The first is always a `MoveTo`, and a segment never directly follows
  /// a `Close`.
  #[must_use]
  pub fn elements(&self) -> &[PathElement] {
    &self.elements
  }

  /// The rule that decides what's inside where the path crosses itself.
  #[must_use]
  pub fn fill_rule(&self) -> FillRule {
    self.fill_rule
  }

  /// The smallest rectangle holding the path: its points and the furthest reach of each curve,
  /// which can be well inside its control points.
  #[must_use]
  pub fn bounds(&self) -> Rect {
    let mut reached = Vec::with_capacity(self.elements.len());
    let (mut figure, mut current) = (Point::ZERO, Point::ZERO);
    for element in self.elements.iter().copied() {
      match element {
        PathElement::MoveTo(point) => figure = point,
        PathElement::QuadTo(control, end) => {
          let at = |t| quad_at(current, control, end, t);
          reached.extend(quad_turns(current.x, control.x, end.x).map(at));
          reached.extend(quad_turns(current.y, control.y, end.y).map(at));
        }
        PathElement::CubicTo(first, second, end) => {
          let at = |t| cubic_at(current, first, second, end, t);
          reached.extend(cubic_turns(current.x, first.x, second.x, end.x).map(at));
          reached.extend(cubic_turns(current.y, first.y, second.y, end.y).map(at));
        }
        PathElement::LineTo(_) | PathElement::Close => {}
      }
      current = element.points().last().unwrap_or(figure);
      reached.push(current);
    }
    let first = reached.first().copied().unwrap_or_default();
    let (min, max) = reached.iter().fold((first, first), |(min, max), point| {
      (
        Point::new(min.x.min(point.x), min.y.min(point.y)),
        Point::new(max.x.max(point.x), max.y.max(point.y)),
      )
    });
    Rect::new(min.x, min.y, max.x - min.x, max.y - min.y)
  }

  /// Whether every point, control points included, is finite: no NaN or infinite coordinate.
  #[must_use]
  pub fn is_finite(&self) -> bool {
    self
      .elements
      .iter()
      .flat_map(|element| element.points())
      .all(|point| point.x.is_finite() && point.y.is_finite())
  }
}

/// Builds a [`Path`] one segment at a time. [`Path::builder`] makes one.
#[derive(Debug, Clone)]
pub struct PathBuilder {
  elements: Vec<PathElement>,
  start: Point,
  closed: bool,
  fill_rule: FillRule,
}

impl PathBuilder {
  /// Starts another figure at `point`.
  #[must_use]
  pub fn move_to(mut self, point: Point) -> PathBuilder {
    self.elements.push(PathElement::MoveTo(point));
    self.start = point;
    self.closed = false;
    self
  }

  /// Adds a straight line to `point`.
  #[must_use]
  pub fn line_to(self, point: Point) -> PathBuilder {
    self.segment(PathElement::LineTo(point))
  }

  /// Adds a quadratic Bézier curve, pulled toward `control`, to `end`.
  #[must_use]
  pub fn quad_to(self, control: Point, end: Point) -> PathBuilder {
    self.segment(PathElement::QuadTo(control, end))
  }

  /// Adds a cubic Bézier curve, pulled toward `first` and then `second`, to `end`.
  #[must_use]
  pub fn cubic_to(self, first: Point, second: Point, end: Point) -> PathBuilder {
    self.segment(PathElement::CubicTo(first, second, end))
  }

  /// Closes the figure with a straight line back to its start. A segment added after it starts
  /// from that start again, as in SVG.
  #[must_use]
  pub fn close(mut self) -> PathBuilder {
    self.elements.push(PathElement::Close);
    self.closed = true;
    self
  }

  /// Sets the rule that decides what's inside where the path crosses itself: `EvenOdd` unless set.
  #[must_use]
  pub fn fill_rule(mut self, fill_rule: FillRule) -> PathBuilder {
    self.fill_rule = fill_rule;
    self
  }

  /// The finished path.
  #[must_use]
  pub fn build(self) -> Path {
    Path {
      elements: self.elements.into(),
      fill_rule: self.fill_rule,
    }
  }

  /// Adds `element` to the figure, first starting it again at its start if it was closed.
  fn segment(self, element: PathElement) -> PathBuilder {
    let start = self.start;
    let mut builder = if self.closed { self.move_to(start) } else { self };
    builder.elements.push(element);
    builder
  }
}

fn quad_turns(start: f32, control: f32, end: f32) -> impl Iterator<Item = f32> {
  let [start, control, end] = [start, control, end].map(f64::from);
  let t = (start - control) / (start - 2.0 * control + end);
  Some(t).filter(|t| (0.0..=1.0).contains(t)).map(|t| t as f32).into_iter()
}

fn cubic_turns(start: f32, first: f32, second: f32, end: f32) -> impl Iterator<Item = f32> {
  let [start, first, second, end] = [start, first, second, end].map(f64::from);
  let (a, b, c) = (first - start, second - first, end - second);
  let (qa, qb, qc) = (a - 2.0 * b + c, 2.0 * (b - a), a);
  let q = -0.5 * (qb + (qb * qb - 4.0 * qa * qc).sqrt().copysign(qb));
  [q / qa, qc / q]
    .into_iter()
    .filter(|t| (0.0..=1.0).contains(t))
    .map(|t| t as f32)
}

fn lerp(from: Point, to: Point, t: f32) -> Point {
  let t = f64::from(t);
  let at = |from: f32, to: f32| (f64::from(from) * (1.0 - t) + f64::from(to) * t) as f32;
  Point::new(at(from.x, to.x), at(from.y, to.y))
}

fn quad_at(start: Point, control: Point, end: Point, t: f32) -> Point {
  lerp(lerp(start, control, t), lerp(control, end, t), t)
}

fn cubic_at(start: Point, first: Point, second: Point, end: Point, t: f32) -> Point {
  lerp(quad_at(start, first, second, t), quad_at(first, second, end, t), t)
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  // The builder records each figure in order. A segment after `close` starts again from the
  // closed figure's start, written out as a `MoveTo`, and `move_to` starts another figure.
  #[test]
  fn path_builder_records_figures() {
    let [a, b, c, d] = [
      Point::new(0.0, 0.0),
      Point::new(10.0, 0.0),
      Point::new(10.0, 10.0),
      Point::new(0.0, 10.0),
    ];

    let path = Path::builder(a)
      .line_to(b)
      .quad_to(c, d)
      .close()
      .line_to(c)
      .move_to(d)
      .cubic_to(a, b, c)
      .build();

    assert_eq!(
      path.elements(),
      [
        PathElement::MoveTo(a),
        PathElement::LineTo(b),
        PathElement::QuadTo(c, d),
        PathElement::Close,
        PathElement::MoveTo(a),
        PathElement::LineTo(c),
        PathElement::MoveTo(d),
        PathElement::CubicTo(a, b, c),
      ]
    );
  }

  // A curve's box reaches as far as the curve does, not as far as its control points.
  #[test]
  fn path_bounds_include_curve_extrema() {
    let quad = Path::builder(Point::new(0.0, 0.0))
      .quad_to(Point::new(5.0, 10.0), Point::new(10.0, 0.0))
      .build();
    let cubic = Path::builder(Point::new(0.0, 0.0))
      .cubic_to(Point::new(0.0, 10.0), Point::new(10.0, 10.0), Point::new(10.0, 0.0))
      .build();
    let lines = Path::builder(Point::new(2.0, 3.0))
      .line_to(Point::new(-1.0, 5.0))
      .line_to(Point::new(4.0, -2.0))
      .close()
      .build();

    assert_eq!(quad.bounds(), Rect::new(0.0, 0.0, 10.0, 5.0));
    assert_eq!(cubic.bounds(), Rect::new(0.0, 0.0, 10.0, 7.5));
    assert_eq!(lines.bounds(), Rect::new(-1.0, -2.0, 5.0, 7.0));
  }

  // A cubic whose turning points are symmetric around zero, so the linear term of its derivative
  // vanishes, still reaches its turn.
  #[test]
  fn path_bounds_reach_a_turn_without_a_linear_term() {
    let cubic = Path::builder(Point::new(0.0, 0.0))
      .cubic_to(Point::new(1.0, 1.0), Point::new(2.0, 2.0), Point::new(3.0, 1.0))
      .build();

    let bounds = cubic.bounds();

    assert!((bounds.height - std::f32::consts::SQRT_2).abs() < 1e-5, "{bounds}");
  }

  // A cubic that rises, dips and rises again turns twice; its box reaches the higher turn, which
  // sits above its end.
  #[test]
  fn path_bounds_reach_the_higher_of_two_turns() {
    let cubic = Path::builder(Point::new(0.0, 0.0))
      .cubic_to(Point::new(1.0, 4.0), Point::new(2.0, 1.0), Point::new(3.0, 2.0))
      .build();

    let bounds = cubic.bounds();

    assert!((bounds.height - 2.151_741_2).abs() < 1e-5, "{bounds}");
  }

  // A quadratic arch raised to a cubic, as SVG and font tools write one, has the quadratic's box,
  // though its cubic term is only rounding noise.
  #[test]
  fn path_bounds_reach_a_nearly_quadratic_peak() {
    for (start, control, end) in [(4.33, 78.0, 3.53), (0.1, 61.7, 0.7), (12.0, -40.3, 11.2)] {
      let [start, control, end] = [Point::new(0.0, start), Point::new(50.0, control), Point::new(100.0, end)];
      let raise = |from: Point| lerp(from, control, 2.0 / 3.0);
      let quad = Path::builder(start).quad_to(control, end).build();
      let cubic = Path::builder(start).cubic_to(raise(start), raise(end), end).build();

      let (expected, actual) = (quad.bounds(), cubic.bounds());

      assert!((expected.height - actual.height).abs() < 1e-3, "{expected} vs {actual}");
    }
  }

  // Curves keep their box when their coordinates are near the largest finite `f32`, where the
  // differences between them would overflow.
  #[test]
  fn path_bounds_hold_for_huge_coordinates() {
    let quad = Path::builder(Point::new(0.0, -f32::MAX))
      .quad_to(Point::new(1.0, f32::MAX), Point::new(2.0, -f32::MAX))
      .build();
    let [low, high] = [-0.5 * f32::MAX, 0.75 * f32::MAX];
    let cubic = Path::builder(Point::new(0.0, low))
      .cubic_to(Point::new(1.0, high), Point::new(2.0, high), Point::new(3.0, low))
      .build();

    assert_eq!(quad.bounds(), Rect::new(0.0, -f32::MAX, 2.0, f32::MAX));
    assert_eq!(cubic.bounds().y, low);
    assert!((cubic.bounds().height / f32::MAX - 0.9375).abs() < 1e-6, "{}", cubic.bounds());
  }

  // A path that is only its start point has a box of no size at that point.
  #[test]
  fn path_with_only_a_start_has_point_bounds() {
    let path = Path::builder(Point::new(3.0, 4.0)).build();

    assert_eq!(path.bounds(), Rect::new(3.0, 4.0, 0.0, 0.0));
  }

  // A path fills by the even-odd rule unless the builder sets another.
  #[test]
  fn path_fill_rule_defaults_to_even_odd() {
    let builder = Path::builder(Point::ZERO).line_to(Point::new(1.0, 1.0));

    assert_eq!(builder.clone().build().fill_rule(), FillRule::EvenOdd);
    assert_eq!(builder.fill_rule(FillRule::NonZero).build().fill_rule(), FillRule::NonZero);
  }

  // A path is finite only if every point, control points included, is.
  #[test]
  fn path_is_finite_checks_every_point() {
    let path = |control: Point| Path::builder(Point::ZERO).quad_to(control, Point::new(1.0, 1.0)).build();

    assert!(path(Point::new(0.5, 2.0)).is_finite());
    assert!(!path(Point::new(f32::NAN, 2.0)).is_finite());
    assert!(!path(Point::new(0.5, f32::NEG_INFINITY)).is_finite());
  }

  // A clone shares its elements rather than copying them.
  #[test]
  fn path_clones_share_elements() {
    let path = Path::builder(Point::ZERO).line_to(Point::new(1.0, 1.0)).build();

    let copy = path.clone();

    assert!(std::ptr::eq(path.elements(), copy.elements()));
  }
}
